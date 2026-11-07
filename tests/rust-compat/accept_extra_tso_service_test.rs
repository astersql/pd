// Copyright 2026 AsterSQL.
use super::{accept_extra_rm_service_test::Sidecar, harness::Server};
use astersql_pd_proto::{pdpb, tsopb as p};
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use prost::Message;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
fn status(error: tonic::Status, cluster: u64) -> Value {
    json!({"code":format!("{:?}",error.code()),"message":error.message().replace(&(cluster+1).to_string(),"<other-cluster>").replace(&cluster.to_string(),"<cluster>")})
}
async fn connect(
    url: &str,
    certs: Option<&Path>,
    identity: Option<&str>,
) -> Result<p::tso_client::TsoClient<tonic::transport::Channel>, String> {
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
    let mut endpoint = Endpoint::from_shared(if certs.is_some() {
        url.replacen("http://", "https://", 1)
    } else {
        url.into()
    })
    .map_err(|e| e.to_string())?
    .connect_timeout(Duration::from_secs(2))
    .timeout(Duration::from_secs(5));
    if let Some(certs) = certs {
        let mut tls = ClientTlsConfig::new()
            .domain_name("localhost")
            .ca_certificate(Certificate::from_pem(
                std::fs::read(certs.join("ca.pem")).unwrap(),
            ));
        if let Some(identity) = identity {
            tls = tls.identity(Identity::from_pem(
                std::fs::read(certs.join(format!("{identity}.pem"))).unwrap(),
                std::fs::read(certs.join(format!("{identity}.key"))).unwrap(),
            ));
        }
        endpoint = endpoint.tls_config(tls).map_err(|e| e.to_string())?;
    }
    Ok(p::tso_client::TsoClient::new(
        endpoint.connect().await.map_err(|e| e.to_string())?,
    ))
}
async fn run(binary: &Path, go: &Path, certs: Option<&Path>) -> Result<Value, String> {
    let mut pd =
        Server::start_with_environment(go, &[], &[("PD_SERVICE_MODE".into(), "api".into())])
            .map_err(|e| e.to_string())?;
    pd.ready()?;
    let mut owner = pdpb::pd_client::PdClient::connect(pd.url.clone())
        .await
        .map_err(|e| e.to_string())?;
    let cluster = owner
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .map_err(|e| e.to_string())?
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let backend = EtcdMetaStore::connect(&[pd.metadata_url().into()], Duration::from_secs(2))
        .await
        .map_err(|e| e.to_string())?;
    let config_arg = certs.map(|certs| {
        let path=certs.join("tso-domain.toml");
        std::fs::write(&path,format!("[security]\ncacert-path={}\ncert-path={}\nkey-path={}\ncert-allowed-cn=[\"allowed\"]\n",json!(certs.join("ca.pem")),json!(certs.join("server.pem")),json!(certs.join("server.key")))).unwrap();
        format!("--config={}",path.display())
    });
    let extra = config_arg.iter().map(String::as_str).collect::<Vec<_>>();
    let mut sidecar =
        Sidecar::start_service_with_args(binary, pd.metadata_url(), "tso", "http", &extra);
    let header = Some(p::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let (mut client, discovery) = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(exit) = sidecar.child.try_wait().map_err(|e| e.to_string())? {
                return Err(format!("startup {exit}: {}", sidecar.logs()));
            }
            if let Ok(mut c) = connect(&sidecar.url, certs, Some("allowed")).await {
                if let Ok(r) = c
                    .find_group_by_keyspace_id(p::FindGroupByKeyspaceIdRequest {
                        header: header.clone(),
                        keyspace: Some(p::find_group_by_keyspace_id_request::Keyspace::KeyspaceId(
                            0,
                        )),
                        ..Default::default()
                    })
                    .await
                {
                    let r = r.into_inner();
                    if r.header.as_ref().is_some_and(|h| h.error.is_none())
                        && r.keyspace_group
                            .as_ref()
                            .is_some_and(|g| g.members.iter().any(|m| m.is_primary))
                    {
                        break Ok((c, r));
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .map_err(|_| format!("TSO readiness: {}", sidecar.logs()))??;
    let paths = astersql_pd_keypath::ClusterPaths::new(cluster);
    let group_key = paths.keyspace_group_id_path(0);
    let group = backend
        .read(Read::range(
            group_key.as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .map_err(|e| e.to_string())?;
    let raw_discovery = discovery.encode_to_vec();
    let mut normalized = discovery.clone();
    normalized.header.as_mut().unwrap().cluster_id = 0;
    for member in &mut normalized.keyspace_group.as_mut().unwrap().members {
        member.address = member.address.replace(&sidecar.url, "<tso>");
    }
    let discovery_revision = normalized.mod_revision;
    normalized.mod_revision = 0;
    let mut frames = vec![];
    let mut signatures = vec![];
    let mut last = (0, 0);
    let (send, receive) = tokio::sync::mpsc::channel(4);
    send.send(p::TsoRequest {
        header: header.clone(),
        count: 1,
        ..Default::default()
    })
    .await
    .map_err(|e| e.to_string())?;
    let mut output = client
        .tso(tokio_stream::wrappers::ReceiverStream::new(receive))
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    for (index, count) in [1, 4, 2].into_iter().enumerate() {
        if index != 0 {
            send.send(p::TsoRequest {
                header: header.clone(),
                count,
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        }
        let r = tokio::time::timeout(Duration::from_secs(5), output.message())
            .await
            .map_err(|_| "timestamp timeout")?
            .map_err(|e| e.to_string())?
            .ok_or("early TSO EOF")?;
        let t = r.timestamp.as_ref().ok_or("missing timestamp")?;
        let now = (t.physical, t.logical);
        if now <= last
            || r.count != count
            || r.header.as_ref().is_none_or(|h| {
                h.error.is_some() || h.cluster_id != cluster || h.keyspace_group_id != 0
            })
        {
            return Err(format!("timestamp semantics {r:?} after {last:?}"));
        }
        if now.0 == last.0 && now.1 - last.1 < i64::from(count) {
            return Err("overlapping timestamp range".into());
        }
        last = now;
        frames.push(r.encode_to_vec());
        let mut n = r;
        n.header.as_mut().unwrap().cluster_id = 0;
        let stamp = n.timestamp.as_mut().unwrap();
        stamp.physical = 0;
        stamp.logical = 0;
        signatures.push(n.encode_to_vec());
    }
    drop(send);
    let end = tokio::time::timeout(Duration::from_secs(5), output.message())
        .await
        .map_err(|_| "TSO close timeout")?;
    let terminal = match end {
        Ok(None) => json!({"kind":"EOF"}),
        Ok(Some(r)) => return Err(format!("unexpected frame {r:?}")),
        Err(e) => status(e, cluster),
    };
    let min = client
        .get_min_ts(p::GetMinTsRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?
        .into_inner();
    let min_raw = min.encode_to_vec();
    let mut min_norm = min.clone();
    min_norm.header.as_mut().unwrap().cluster_id = 0;
    let min_stamp = min_norm.timestamp.as_mut().ok_or("missing min timestamp")?;
    if (min_stamp.physical, min_stamp.logical) <= last {
        return Err(format!("minimum timestamp does not advance {min:?}"));
    }
    min_stamp.physical = 0;
    min_stamp.logical = 0;
    let mut errors = vec![];
    let mut raw_errors = vec![];
    for invalid in [
        None,
        Some(p::RequestHeader {
            cluster_id: cluster + 1,
            ..Default::default()
        }),
    ] {
        let find = client
            .find_group_by_keyspace_id(p::FindGroupByKeyspaceIdRequest {
                header: invalid.clone(),
                ..Default::default()
            })
            .await;
        let min = client
            .get_min_ts(p::GetMinTsRequest {
                header: invalid.clone(),
                ..Default::default()
            })
            .await;
        fn response<M: Message + std::fmt::Debug>(
            r: Result<tonic::Response<M>, tonic::Status>,
            cluster: u64,
        ) -> Value {
            match r {
                Ok(r) => {
                    json!({"response":format!("{:?}",r.into_inner()).replace(&cluster.to_string(),"<cluster>").replace(&(cluster+1).to_string(),"<other-cluster>")})
                }
                Err(e) => status(e, cluster),
            }
        }
        let find_raw = format!("{find:?}");
        let min_raw = format!("{min:?}");
        let bad = client
            .tso(tokio_stream::iter([p::TsoRequest {
                header: invalid,
                count: 1,
                ..Default::default()
            }]))
            .await;
        let outcome = match bad {
            Err(e) => Err(e),
            Ok(r) => tokio::time::timeout(Duration::from_secs(5), r.into_inner().message())
                .await
                .map_err(|_| "invalid stream timeout")?,
        };
        let tso_raw = format!("{outcome:?}");
        let tso = match outcome {
            Err(e) => status(e, cluster),
            Ok(r) => json!({"unexpected":format!("{r:?}")}),
        };
        raw_errors.push(json!({"find":find_raw,"min":min_raw,"tso":tso_raw}));
        errors.push(json!({"find":response(find,cluster),"min":response(min,cluster),"tso":tso}));
    }
    let time_key = paths.timestamp_path(0);
    let persisted = backend
        .read(Read::prefix(time_key.as_bytes(), Consistency::Linearizable))
        .await
        .map_err(|e| e.to_string())?;
    let mut refusals = vec![];
    if let Some(certs) = certs {
        for identity in [None, Some("denied"), Some("rogue")] {
            let response = match connect(&sidecar.url, Some(certs), identity).await {
                Ok(mut client) => client
                    .get_min_ts(p::GetMinTsRequest {
                        header: header.clone(),
                        ..Default::default()
                    })
                    .await
                    .map(|r| format!("{:?}", r.into_inner()))
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            refusals.push(json!({"identity":identity,"accepted":response.is_ok(),"raw":format!("{response:?}")}));
        }
    }
    sidecar.stop();
    let after = backend
        .read(Read::range(
            group_key.as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .map_err(|e| e.to_string())?;
    let row = json!({"discovery_raw":raw_discovery,"discovery_normalized":normalized.encode_to_vec(),"discovery_revision":discovery_revision,"group_mod_revision":group.entries.first().map(|e|e.mod_revision),"group_unchanged":group.entries==after.entries,"frames":frames,"timestamp_semantics":signatures,"terminal":terminal,"minimum_raw":min_raw,"minimum_normalized":min_norm.encode_to_vec(),"tls_refusals":refusals,"errors":errors,"errors_raw":raw_errors,"persisted_timestamps":persisted.entries.iter().map(|e|json!({"key":e.key,"value":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>(),"log":sidecar.logs()});
    drop(backend);
    drop(owner);
    pd.stop()?;
    Ok(row)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_tso_three_methods_real_timestamps_discovery_and_errors() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut rows = vec![];
    let certs = super::accept_extra_transport_config_test::certificates("tso-domain");
    for (binary, is_rust, tls) in [
        (&go, false, false),
        (&rust, true, false),
        (&go, false, true),
        (&rust, true, true),
    ] {
        let r = tokio::time::timeout(
            Duration::from_secs(40),
            run(binary, &go, tls.then_some(certs.0.as_path())),
        )
        .await;
        let mut row = match r {
            Ok(Ok(row)) => row,
            other => json!({"error":format!("{other:?}")}),
        };
        row["rust"] = json!(is_rust);
        row["tls"] = json!(tls);
        rows.push(row);
        std::fs::write(
            PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
                .with_extension("tso-methods.json"),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
    }
    for row in &rows {
        assert!(row.get("error").is_none(), "{row}");
        assert_eq!(row["group_unchanged"], true);
        assert!(!row["persisted_timestamps"].as_array().unwrap().is_empty());
        for refusal in row["tls_refusals"].as_array().unwrap() {
            assert_eq!(refusal["accepted"], false, "{refusal}");
        }
        assert_eq!(
            row["discovery_revision"].as_i64().unwrap(),
            row["group_mod_revision"].as_i64().unwrap_or(0)
        );
    }
    for field in [
        "discovery_normalized",
        "timestamp_semantics",
        "terminal",
        "minimum_normalized",
        "errors",
    ] {
        for pair in [[0, 1], [2, 3]] {
            assert_eq!(rows[pair[0]][field], rows[pair[1]][field], "{field}");
        }
    }
}

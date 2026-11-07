// Copyright 2026 AsterSQL.
use super::{accept_extra_transport_config_test::Work, accept_http_10_test::http, harness::Server};
use astersql_pd_proto::{keyspacepb as p, metapb, pdpb};
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use prost::Message;
use serde_json::json;
use std::{path::PathBuf, time::Duration};
async fn message(
    stream: &mut tonic::Streaming<p::WatchKeyspacesResponse>,
    state: i32,
) -> (Vec<u8>, Vec<u8>) {
    let response = tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let raw = response.encode_to_vec();
    let mut normalized = response;
    assert!(
        normalized
            .keyspaces
            .iter()
            .any(|k| k.name == "watch_16" && k.state == state),
        "{normalized:?}"
    );
    normalized.header.as_mut().unwrap().cluster_id = 0;
    for k in &mut normalized.keyspaces {
        k.created_at = 0;
        k.state_changed_at = 0;
    }
    (raw, normalized.encode_to_vec())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_keyspace_watch_cancel_reopen_and_live_backend_compaction() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let work = Work(std::env::temp_dir().join(format!("pd-keyspace-watch-{}", std::process::id())));
    std::fs::create_dir(&work.0).unwrap();
    let config = work.0.join("pd.toml");
    std::fs::write(&config, "[keyspace]\nwait-region-split=false\n").unwrap();
    let flag = format!("--config={}", config.display());
    let mut rows = vec![];
    for is_rust in [false, true] {
        let mut server = if is_rust {
            Server::start_rust_with_args(&rust, &[&flag])
        } else {
            Server::start_with_args(&go, &[&flag])
        }
        .unwrap();
        server.ready().unwrap();
        let mut pd = pdpb::pd_client::PdClient::connect(server.url.clone())
            .await
            .unwrap();
        let cluster = pd
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let header = Some(pdpb::RequestHeader {
            cluster_id: cluster,
            ..Default::default()
        });
        assert!(
            pd.bootstrap(pdpb::BootstrapRequest {
                header: header.clone(),
                store: Some(metapb::Store {
                    id: 1,
                    address: "keyspace-watch-store".into(),
                    ..Default::default()
                }),
                region: Some(metapb::Region {
                    id: 2,
                    peers: vec![metapb::Peer {
                        id: 3,
                        store_id: 1,
                        ..Default::default()
                    }],
                    ..Default::default()
                })
            })
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_none()
        );
        let created = http(
            &server.url,
            "POST",
            "v2/keyspaces/id",
            r#"{"id":10,"name":"watch_16"}"#,
        );
        assert_eq!(created["status"], 200, "{created}");
        let backend =
            EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(2))
                .await
                .unwrap();
        let paths = astersql_pd_keypath::ClusterPaths::new(cluster);
        let mut client = p::keyspace_client::KeyspaceClient::connect(server.url.clone())
            .await
            .unwrap();
        let mut loaded = client
            .load_keyspace(p::LoadKeyspaceRequest {
                header: header.clone(),
                name: "watch_16".into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let load_before = loaded.encode_to_vec();
        assert!(loaded.keyspace.is_none());
        assert_eq!(
            loaded
                .header
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .r#type,
            pdpb::ErrorType::EntryNotFound as i32
        );
        loaded.header.as_mut().unwrap().cluster_id = 0;
        let load_before_normalized = loaded.encode_to_vec();
        let bound = astersql_pd_keyspace::make_region_bound(10);
        let (left, right) = astersql_pd_keyspace::BoundType::Txn.bounds(&bound);
        let allocation = pd
            .alloc_id(pdpb::AllocIdRequest {
                header: header.clone(),
                count: 8,
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(allocation.count, 8);
        let last = allocation.id;
        let requests = [
            (last - 3, last - 2, left.to_vec(), right.to_vec()),
            (last - 1, last, right.to_vec(), vec![]),
        ]
        .into_iter()
        .map(|(id, peer_id, start_key, end_key)| {
            let peer = metapb::Peer {
                id: peer_id,
                store_id: 1,
                ..Default::default()
            };
            pdpb::RegionHeartbeatRequest {
                header: header.clone(),
                region: Some(metapb::Region {
                    id,
                    start_key,
                    end_key,
                    region_epoch: Some(metapb::RegionEpoch {
                        version: 100,
                        conf_ver: 1,
                    }),
                    peers: vec![peer.clone()],
                    ..Default::default()
                }),
                leader: Some(peer),
                approximate_size: 1024 * 1024,
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();
        let boundary_requests = requests
            .iter()
            .map(Message::encode_to_vec)
            .collect::<Vec<_>>();
        let boundary_terminal = match pd.region_heartbeat(tokio_stream::iter(requests)).await {
            Err(e) => format!("{e:?}"),
            Ok(r) => {
                let mut stream = r.into_inner();
                loop {
                    match tokio::time::timeout(Duration::from_secs(5), stream.message())
                        .await
                        .unwrap()
                    {
                        Ok(Some(_)) => {}
                        other => break format!("{other:?}"),
                    }
                }
            }
        };
        for key in [left, right] {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let response = pd
                        .get_region(pdpb::GetRegionRequest {
                            header: header.clone(),
                            region_key: key.to_vec(),
                            ..Default::default()
                        })
                        .await
                        .unwrap()
                        .into_inner();
                    if response.region.as_ref().is_some_and(|r| r.start_key == key) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
        }
        let mut loaded = client
            .load_keyspace(p::LoadKeyspaceRequest {
                header: header.clone(),
                name: "watch_16".into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let load_raw = loaded.encode_to_vec();
        assert_eq!(loaded.keyspace.as_ref().unwrap().name, "watch_16");
        loaded.header.as_mut().unwrap().cluster_id = 0;
        loaded.keyspace.as_mut().unwrap().created_at = 0;
        loaded.keyspace.as_mut().unwrap().state_changed_at = 0;
        let mut by_id = client
            .load_keyspace_by_id(p::LoadKeyspaceByIdRequest {
                header: header.clone(),
                keyspace: Some(p::load_keyspace_by_id_request::Keyspace::Id(10)),
            })
            .await
            .unwrap()
            .into_inner();
        let by_id_raw = by_id.encode_to_vec();
        by_id.header.as_mut().unwrap().cluster_id = 0;
        by_id.keyspace.as_mut().unwrap().created_at = 0;
        by_id.keyspace.as_mut().unwrap().state_changed_at = 0;
        assert_eq!(loaded, by_id);
        let mut all = client
            .get_all_keyspaces(p::GetAllKeyspacesRequest {
                header: header.clone(),
                limit: 20,
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        let all_raw = all.encode_to_vec();
        assert!(all.keyspaces.iter().any(|k| k.name == "watch_16"));
        all.header.as_mut().unwrap().cluster_id = 0;
        for k in &mut all.keyspaces {
            k.created_at = 0;
            k.state_changed_at = 0;
        }
        let lookup = client
            .lookup_keyspace(p::LookupKeyspaceRequest {
                header: header.clone(),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(lookup.code(), tonic::Code::Unimplemented);
        let unaries = json!({"load_before_raw":load_before,"load_before_normalized":load_before_normalized,"boundary_requests":boundary_requests,"boundary_terminal_owned_by22":boundary_terminal,"load_raw":load_raw,"load_normalized":loaded.encode_to_vec(),"by_id_raw":by_id_raw,"all_raw":all_raw,"all_normalized":all.encode_to_vec(),"lookup":{"code":format!("{:?}",lookup.code()),"message":lookup.message()}});
        let mut first = client
            .watch_keyspaces(p::WatchKeyspacesRequest {
                header: header.clone(),
            })
            .await
            .unwrap()
            .into_inner();
        let initial = message(&mut first, 0).await;
        drop(first);
        let response = client
            .update_keyspace_state(p::UpdateKeyspaceStateRequest {
                header: header.clone(),
                keyspace: Some(p::update_keyspace_state_request::Keyspace::Id(10)),
                state: 1,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
        let mut reopened = client
            .watch_keyspaces(p::WatchKeyspacesRequest {
                header: header.clone(),
            })
            .await
            .unwrap()
            .into_inner();
        let refreshed = message(&mut reopened, 1).await;
        let snapshot = backend
            .read(Read::prefix(
                paths.keyspace_meta_prefix().as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        backend.compact(snapshot.revision).await.unwrap();
        let response = client
            .update_keyspace_state(p::UpdateKeyspaceStateRequest {
                header: header.clone(),
                keyspace: Some(p::update_keyspace_state_request::Keyspace::Id(10)),
                state: 2,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(response.header.unwrap().error.is_none());
        let after_compaction = message(&mut reopened, 2).await;
        drop(reopened);
        let mut final_stream = client
            .watch_keyspaces(p::WatchKeyspacesRequest { header })
            .await
            .unwrap()
            .into_inner();
        let final_snapshot = message(&mut final_stream, 2).await;
        drop(final_stream);
        let persisted = backend
            .read(Read::prefix(
                paths.keyspace_meta_prefix().as_bytes(),
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        rows.push(json!({"rust":is_rust,"unaries":unaries,"initial":{"raw":initial.0,"normalized":initial.1},"reopened":{"raw":refreshed.0,"normalized":refreshed.1},"after_compaction":{"raw":after_compaction.0,"normalized":after_compaction.1},"final_snapshot":{"raw":final_snapshot.0,"normalized":final_snapshot.1},"compacted_revision":snapshot.revision,"persisted":persisted.entries.iter().map(|e|json!({"key":e.key,"value":e.value,"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>(),"boundary":"Real frontend client cancellation/drop followed by actual domain update and new snapshot. Compaction occurs while current frontend stream is live; not a forced stale backend cursor. WatchKeyspaces has no request cursor or CloseSend, no normal finite EOF; process terminal belongs to acceptance22."}));
        std::fs::write(
            PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
                .with_extension("keyspace-watch.json"),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
        drop(client);
        drop(pd);
        drop(backend);
        server.stop().unwrap();
    }
    for field in [
        "load_before_normalized",
        "load_normalized",
        "all_normalized",
        "lookup",
    ] {
        assert_eq!(
            rows[0]["unaries"][field], rows[1]["unaries"][field],
            "{field}"
        );
    }
    for phase in ["initial", "reopened", "after_compaction", "final_snapshot"] {
        assert_eq!(
            rows[0][phase]["normalized"], rows[1][phase]["normalized"],
            "{phase}"
        );
    }
}

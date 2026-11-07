// Copyright 2026 AsterSQL.
use super::{
    accept_extra_transport_config_test::certificates, accept_modes::Deployment, harness::Server,
};
use astersql_pd_proto::resource_manager as p;
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use prost::Message;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
async fn connect(
    url: &str,
    certs: &Path,
    identity: Option<&str>,
) -> Result<p::resource_manager_client::ResourceManagerClient<tonic::transport::Channel>, String> {
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
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
    let channel = Endpoint::from_shared(url.replacen("http://", "https://", 1))
        .unwrap()
        .tls_config(tls)
        .unwrap()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(5))
        .connect()
        .await
        .map_err(|e| e.to_string())?;
    Ok(p::resource_manager_client::ResourceManagerClient::new(
        channel,
    ))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_business_mutual_tls_reads_rejected_writes_and_real_token_balance() {
    matrix().await;
}
pub(super) async fn matrix() {
    let certs = certificates("rm-domain");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let owner_cfg = certs.0.join("owner.toml");
    std::fs::write(
        &owner_cfg,
        "[micro-service]\nenable-resource-manager-fallback=false\n",
    )
    .unwrap();
    let owner_flag = format!("--config={}", owner_cfg.display());
    let service_cfg = certs.0.join("rm.toml");
    std::fs::write(&service_cfg,format!("[security]\ncacert-path={}\ncert-path={}\nkey-path={}\ncert-allowed-cn=[\"allowed\"]\n",json!(certs.0.join("ca.pem")),json!(certs.0.join("server.pem")),json!(certs.0.join("server.key")))).unwrap();
    let service_flag = format!("--config={}", service_cfg.display());
    let mut rows = vec![];
    for (binary, is_rust, embedded) in [
        (&go, false, false),
        (&rust, true, false),
        (&rust, true, true),
    ] {
        let mut owner = Server::start_with_environment(
            &go,
            &[&owner_flag],
            &[("PD_SERVICE_MODE".into(), "api".into())],
        )
        .unwrap();
        owner.ready().unwrap();
        let mut metadata =
            p::resource_manager_client::ResourceManagerClient::connect(owner.url.clone())
                .await
                .unwrap();
        let scope = Some(p::KeyspaceIdValue {
            keyspace: Some(p::keyspace_id_value::Keyspace::Value(7)),
        });
        let group = p::ResourceGroup {
            name: "tls16".into(),
            mode: 1,
            priority: 8,
            keyspace_id: scope.clone(),
            r_u_settings: Some(p::GroupRequestUnitSettings {
                r_u: Some(p::TokenBucket {
                    settings: Some(p::TokenLimitSettings {
                        fill_rate: 0,
                        burst_limit: 2000,
                        max_tokens: 0.0,
                    }),
                    tokens: 25.0,
                }),
            }),
            ..Default::default()
        };
        metadata
            .add_resource_group(p::PutResourceGroupRequest {
                group: Some(group.clone()),
            })
            .await
            .unwrap();
        let backend =
            EtcdMetaStore::connect(&[owner.metadata_url().into()], Duration::from_secs(2))
                .await
                .unwrap();
        let key = astersql_pd_keypath::keyspace_resource_group_setting_path(7, "tls16");
        let state_key = astersql_pd_keypath::keyspace_resource_group_state_path(7, "tls16");
        let before = backend
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        let cluster = astersql_pd_proto::pdpb::pd_client::PdClient::connect(owner.url.clone())
            .await
            .unwrap()
            .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let mut sidecar = Deployment::start(
            binary,
            owner.metadata_url(),
            "resource-manager",
            cluster,
            &[&service_flag],
            embedded,
            Some(certs.0.as_path()),
        )
        .await;
        let (mut client, get) = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Ok(mut c) = connect(&sidecar.url, &certs.0, Some("allowed")).await {
                    if let Ok(r) = c
                        .get_resource_group(p::GetResourceGroupRequest {
                            resource_group_name: "tls16".into(),
                            keyspace_id: scope.clone(),
                            ..Default::default()
                        })
                        .await
                    {
                        break (c, r.into_inner());
                    }
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        let list = client
            .list_resource_groups(p::ListResourceGroupsRequest {
                keyspace_id: scope.clone(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(list.groups.iter().any(|g| g.name == "tls16"));
        let add = client
            .add_resource_group(p::PutResourceGroupRequest {
                group: Some(group.clone()),
            })
            .await
            .unwrap_err();
        let modify = client
            .modify_resource_group(p::PutResourceGroupRequest { group: Some(group) })
            .await
            .unwrap_err();
        let delete = client
            .delete_resource_group(p::DeleteResourceGroupRequest {
                resource_group_name: "tls16".into(),
                keyspace_id: scope.clone(),
            })
            .await
            .unwrap_err();
        let errors = [add, modify, delete]
            .iter()
            .map(|e| json!({"code":format!("{:?}",e.code()),"message":e.message()}))
            .collect::<Vec<_>>();
        let request = p::TokenBucketsRequest {
            requests: vec![p::TokenBucketRequest {
                resource_group_name: "tls16".into(),
                keyspace_id: scope.clone(),
                request: Some(p::token_bucket_request::Request::RuItems(
                    p::token_bucket_request::RequestRu {
                        request_r_u: vec![p::RequestUnitItem {
                            r#type: 0,
                            value: 5.0,
                        }],
                    },
                )),
                ..Default::default()
            }],
            target_request_period_ms: 5000,
            client_unique_id: 23,
        };
        let mut stream = client
            .acquire_token_buckets(tokio_stream::iter([request.clone()]))
            .await
            .unwrap()
            .into_inner();
        let token = tokio::time::timeout(Duration::from_secs(5), stream.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            token.responses[0].granted_r_u_tokens[0]
                .granted_tokens
                .as_ref()
                .unwrap()
                .tokens,
            5.0
        );
        assert!(stream.message().await.unwrap().is_none());
        let mut refusals = vec![];
        for identity in [None, Some("denied"), Some("rogue")] {
            let result = match connect(&sidecar.url, &certs.0, identity).await {
                Ok(mut c) => c
                    .get_resource_group(p::GetResourceGroupRequest {
                        resource_group_name: "tls16".into(),
                        keyspace_id: scope.clone(),
                        ..Default::default()
                    })
                    .await
                    .map(|r| format!("{:?}", r.into_inner()))
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            assert!(result.is_err());
            refusals
                .push(json!({"identity":identity,"raw":format!("{result:?}"),"accepted":false}));
        }
        let balance = client
            .get_resource_group(p::GetResourceGroupRequest {
                resource_group_name: "tls16".into(),
                keyspace_id: scope.clone(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            balance
                .group
                .as_ref()
                .unwrap()
                .r_u_settings
                .as_ref()
                .unwrap()
                .r_u
                .as_ref()
                .unwrap()
                .tokens,
            99995.0
        );
        sidecar.stop().await;
        let after = backend
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        assert_eq!(before.entries, after.entries);
        let state = backend
            .read(Read::range(
                state_key.as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        assert!(
            state.entries.is_empty(),
            "this bounded TLS case ends before the 60s state save; periodic durability is covered by mcs138"
        );
        rows.push(json!({"rust":is_rust,"embedded":embedded,"get":get.encode_to_vec(),"list":list.encode_to_vec(),"rejected_writes":errors,"token_request":request.encode_to_vec(),"token_response":token.encode_to_vec(),"tls_refusals":refusals,"metadata_unchanged":before.entries==after.entries,"settings_bytes":before.entries[0].value,"balance_after_consumption":balance.encode_to_vec(),"state_entries":state.entries.iter().map(|e|json!({"key":e.key,"value":e.value,"mod_revision":e.mod_revision})).collect::<Vec<_>>(),"log":sidecar.logs()}));
        std::fs::write(
            PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
                .with_extension("rm-domain-tls.json"),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
        drop(client);
        drop(metadata);
        drop(backend);
        owner.stop().unwrap();
    }
    for field in [
        "get",
        "list",
        "rejected_writes",
        "token_request",
        "token_response",
        "settings_bytes",
        "balance_after_consumption",
        "state_entries",
    ] {
        for index in [1, 2] {
            assert_eq!(rows[0][field], rows[index][field], "{field}");
        }
    }
}

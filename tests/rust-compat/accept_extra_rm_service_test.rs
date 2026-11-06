// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_proto::{
    pdpb,
    resource_manager::{self as p, resource_manager_client::ResourceManagerClient},
};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use astersql_pd_storage_kv::EtcdMetaStore;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct File(PathBuf);
impl Drop for File {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
struct Sidecar {
    child: Child,
    log: File,
    url: String,
}
impl Sidecar {
    fn start(binary: &Path, backend: &str) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let path = std::env::temp_dir().join(format!(
            "pd-sa016-rm-{}-{}.log",
            std::process::id(),
            url.rsplit(':').next().unwrap()
        ));
        let log = File(path);
        let output = std::fs::File::create(&log.0).unwrap();
        let mut command = Command::new(binary);
        command
            .args([
                "services",
                "resource-manager",
                &format!("--backend-endpoints={backend}"),
                &format!("--listen-addr={url}"),
                &format!("--advertise-listen-addr={url}"),
                "--name=accept-rm",
            ])
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output));
        drop(listener);
        let child = command.spawn().unwrap();
        Self { child, log, url }
    }
    fn logs(&self) -> String {
        std::fs::read_to_string(&self.log.0).unwrap_or_default()
    }
    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            assert!(
                Command::new("kill")
                    .args(["-TERM", &self.child.id().to_string()])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "sidecar exited: {status}: {}",
                    self.logs()
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "sidecar shutdown timeout: {}",
                self.logs()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Sidecar {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
fn save(records: &[Value]) {
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap());
    let path = path.with_file_name(format!(
        "{}-service.json",
        path.file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(path, serde_json::to_vec(records).unwrap()).unwrap();
}
fn scoped() -> Option<p::KeyspaceIdValue> {
    Some(p::KeyspaceIdValue {
        keyspace: Some(p::keyspace_id_value::Keyspace::Value(7)),
    })
}
fn group(priority: u32, tokens: f64) -> p::ResourceGroup {
    p::ResourceGroup {
        name: "watched".into(),
        mode: 1,
        priority,
        keyspace_id: scoped(),
        r_u_settings: Some(p::GroupRequestUnitSettings {
            r_u: Some(p::TokenBucket {
                settings: Some(p::TokenLimitSettings {
                    fill_rate: 500,
                    burst_limit: 2000,
                    max_tokens: 0.0,
                }),
                tokens,
            }),
        }),
        ..Default::default()
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_standalone_metadata_watch() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let config =
        File(std::env::temp_dir().join(format!("pd-sa016-rm-api-{}.toml", std::process::id())));
    std::fs::write(
        &config.0,
        "[micro-service]\nenable-resource-manager-fallback = false\n",
    )
    .unwrap();
    let arg = format!("--config={}", config.0.display());
    let mut records = Vec::new();
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        // The upstream metadata owner is the same existing Go PD mode on both sides.
        let mut pd = Server::start_with_args(&go, &[&arg]).unwrap();
        pd.ready().unwrap();
        let mut metadata = ResourceManagerClient::connect(pd.url.clone())
            .await
            .unwrap();
        metadata
            .add_resource_group(p::PutResourceGroupRequest {
                group: Some(group(9, 75.0)),
            })
            .await
            .unwrap();
        let mut identity = pdpb::pd_client::PdClient::connect(pd.url.clone())
            .await
            .unwrap();
        let cluster = identity
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let store = EtcdMetaStore::connect(&[pd.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        let mut child = Sidecar::start(binary, pd.metadata_url());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let mut client = loop {
            if let Some(status) = child.child.try_wait().unwrap() {
                records.push(json!({"rust":is_rust,"stage":"startup","exit":status.to_string(),"log":child.logs()}));
                save(&records);
                panic!(
                    "standalone RM must stay alive and serve: {status}: {}",
                    child.logs()
                );
            }
            if let Ok(mut client) = ResourceManagerClient::connect(child.url.clone()).await {
                if client
                    .list_resource_groups(p::ListResourceGroupsRequest::default())
                    .await
                    .is_ok()
                {
                    break client;
                }
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "standalone RM not ready: {}",
                child.logs()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        let request = p::GetResourceGroupRequest {
            resource_group_name: "watched".into(),
            keyspace_id: scoped(),
            ..Default::default()
        };
        let response = client
            .get_resource_group(request.clone())
            .await
            .unwrap()
            .into_inner();
        records.push(
            json!({"rust":is_rust,"stage":"initial snapshot","response":format!("{response:?}")}),
        );
        save(&records);
        let initial = response.group.unwrap();
        assert_eq!(initial.priority, 9);
        assert_eq!(initial.r_u_settings.unwrap().r_u.unwrap().tokens, 75.0);
        let path = ClusterPaths::new(cluster).election_path(Some(&MsParam {
            service_name: "resource_manager".into(),
            group_id: 0,
        }));
        let primary = store
            .read(Read::range(path.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        use prost::Message;
        let primary = p::Participant::decode(primary.entries[0].value.as_slice()).unwrap();
        let digest = Sha256::digest(child.url.as_bytes());
        assert_eq!(
            primary.id,
            u64::from_le_bytes(digest[..8].try_into().unwrap())
        );
        assert_eq!(primary.name, child.url);
        assert_eq!(primary.listen_urls, [child.url.clone()]);
        let error = client
            .add_resource_group(p::PutResourceGroupRequest {
                group: Some(group(9, 0.0)),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::FailedPrecondition);
        assert_eq!(
            error.message(),
            "resource group metadata writes must be handled by PD"
        );
        records.push(json!({"rust":is_rust,"stage":"metadata write rejected","error":format!("{error:?}"),"participant":format!("{primary:?}")}));
        save(&records);
        for error in [
            client
                .modify_resource_group(p::PutResourceGroupRequest {
                    group: Some(group(15, 100.0)),
                })
                .await
                .unwrap_err(),
            client
                .delete_resource_group(p::DeleteResourceGroupRequest {
                    resource_group_name: "watched".into(),
                    keyspace_id: scoped(),
                })
                .await
                .unwrap_err(),
        ] {
            assert_eq!(error.code(), tonic::Code::FailedPrecondition);
            assert_eq!(
                error.message(),
                "resource group metadata writes must be handled by PD"
            );
            records.push(json!({"rust":is_rust,"stage":"metadata write rejected","error":format!("{error:?}")}));
        }
        metadata
            .modify_resource_group(p::PutResourceGroupRequest {
                group: Some(group(12, 25.0)),
            })
            .await
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let response = client
                .get_resource_group(request.clone())
                .await
                .unwrap()
                .into_inner()
                .group
                .unwrap();
            records.push(
                json!({"rust":is_rust,"stage":"settings watch","response":format!("{response:?}")}),
            );
            save(&records);
            if response.priority == 12 {
                assert_eq!(
                    response.r_u_settings.unwrap().r_u.unwrap().tokens,
                    75.0,
                    "watch applies settings without adding metadata token deltas"
                );
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "live settings watch did not update priority"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let (send, receive) = tokio::sync::mpsc::channel(2);
        send.send(p::TokenBucketsRequest {
            requests: vec![p::TokenBucketRequest {
                resource_group_name: "watched".into(),
                keyspace_id: scoped(),
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
        })
        .await
        .unwrap();
        let mut tokens = client
            .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(receive))
            .await
            .unwrap()
            .into_inner();
        let granted = tokio::time::timeout(Duration::from_secs(3), tokens.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(granted.responses.len(), 1);
        assert_eq!(
            granted.responses[0].granted_r_u_tokens[0]
                .granted_tokens
                .as_ref()
                .unwrap()
                .tokens,
            5.0
        );
        records.push(json!({"rust":is_rust,"stage":"standalone token allocation","response":format!("{granted:?}")}));
        save(&records);
        metadata
            .delete_resource_group(p::DeleteResourceGroupRequest {
                resource_group_name: "watched".into(),
                keyspace_id: scoped(),
            })
            .await
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            match client.get_resource_group(request.clone()).await {
                Err(error) => {
                    assert_eq!(error.code(), tonic::Code::Unknown);
                    assert!(error.message().contains("resource group does not exist"));
                    records.push(
                        json!({"rust":is_rust,"stage":"delete watch","error":format!("{error:?}")}),
                    );
                    save(&records);
                    break;
                }
                Ok(_) => assert!(
                    tokio::time::Instant::now() < deadline,
                    "deleted resource group remains cached"
                ),
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        child.stop();
        let ended = tokio::time::timeout(Duration::from_secs(3), tokens.message())
            .await
            .expect("shutdown must terminate retained token stream");
        assert!(!matches!(ended, Ok(Some(_))));
        records.push(json!({"rust":is_rust,"stage":"shutdown retained stream","response":format!("{ended:?}")}));
        save(&records);
        drop(send);
        pd.stop().unwrap();
    }
}

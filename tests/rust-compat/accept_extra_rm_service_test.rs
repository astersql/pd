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
pub(super) struct Sidecar {
    pub(super) child: Child,
    log: File,
    pub(super) url: String,
}
impl Sidecar {
    fn start(binary: &Path, backend: &str) -> Self {
        Self::start_with_scheme(binary, backend, "http")
    }
    fn start_with_scheme(binary: &Path, backend: &str, scheme: &str) -> Self {
        Self::start_service(binary, backend, "resource-manager", scheme)
    }
    pub(super) fn start_service(binary: &Path, backend: &str, service: &str, scheme: &str) -> Self {
        Self::start_service_with_args(binary, backend, service, scheme, &[])
    }
    pub(super) fn start_service_with_args(
        binary: &Path,
        backend: &str,
        service: &str,
        scheme: &str,
        extra: &[&str],
    ) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let advertised = url.replacen("http://", &format!("{scheme}://"), 1);
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
                service,
                &format!("--backend-endpoints={backend}"),
                &format!("--listen-addr={url}"),
                &format!("--advertise-listen-addr={advertised}"),
                "--name=accept-rm",
            ])
            .args(extra)
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output));
        drop(listener);
        let child = command.spawn().unwrap();
        Self { child, log, url }
    }
    pub(super) fn logs(&self) -> String {
        std::fs::read_to_string(&self.log.0).unwrap_or_default()
    }
    pub(super) fn stop(&mut self) {
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
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_classic_proxy_routes_reads_and_preserves_metadata_ownership() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let config =
        File(std::env::temp_dir().join(format!("pd-sa016-rm-proxy-{}.toml", std::process::id())));
    std::fs::write(
        &config.0,
        "[micro-service]\nenable-resource-manager-fallback = false\n",
    )
    .unwrap();
    let argument = format!("--config={}", config.0.display());
    let mut records = Vec::new();
    for (binary, is_rust, scheme) in [
        (&go, false, "http"),
        (&rust, true, "http"),
        (&go, false, "https"),
        (&rust, true, "https"),
    ] {
        records.push(json!({"rust":is_rust,"advertised_scheme":scheme,"stage":"scenario"}));
        save(&records);
        let environment = [("PD_SERVICE_MODE".into(), "api".into())];
        let mut pd = if is_rust {
            Server::start_rust_with_environment(binary, &[&argument], &environment)
        } else {
            Server::start_with_environment(binary, &[&argument], &environment)
        }
        .unwrap();
        pd.ready().unwrap();
        let mut client = ResourceManagerClient::connect(pd.url.clone())
            .await
            .unwrap();
        let error = client
            .list_resource_groups(p::ListResourceGroupsRequest::default())
            .await
            .unwrap_err();
        records
            .push(json!({"rust":is_rust,"stage":"proxy no primary","error":format!("{error:?}")}));
        save(&records);
        assert_eq!(error.code(), tonic::Code::Unavailable);
        assert_eq!(error.message(), "resource manager service is not available");
        for error in [
            client
                .add_resource_group(p::PutResourceGroupRequest::default())
                .await
                .unwrap_err(),
            client
                .modify_resource_group(p::PutResourceGroupRequest::default())
                .await
                .unwrap_err(),
        ] {
            assert_eq!(error.code(), tonic::Code::InvalidArgument);
            assert_eq!(error.message(), "resource group is required");
        }
        let error = client
            .delete_resource_group(p::DeleteResourceGroupRequest::default())
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert_eq!(error.message(), "resource group name is required");
        client
            .add_resource_group(p::PutResourceGroupRequest {
                group: Some(group(9, 75.0)),
            })
            .await
            .unwrap();
        let store = EtcdMetaStore::connect(&[pd.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        let setting = store
            .read(Read::range(
                astersql_pd_keypath::keyspace_resource_group_setting_path(7, "watched").as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        let state = store
            .read(Read::range(
                astersql_pd_keypath::keyspace_resource_group_state_path(7, "watched").as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        records.push(json!({"rust":is_rust,"stage":"metadata-only PD write","settings":format!("{setting:?}"),"state":format!("{state:?}")}));
        save(&records);
        assert_eq!(setting.entries.len(), 1);
        assert!(
            state.entries.is_empty(),
            "metadata-only PD must not create token state"
        );
        let mut sidecar = Sidecar::start_with_scheme(&go, pd.metadata_url(), scheme);
        let request = p::GetResourceGroupRequest {
            resource_group_name: "watched".into(),
            keyspace_id: scoped(),
            ..Default::default()
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            match client.get_resource_group(request.clone()).await {
                Ok(response) => {
                    assert_eq!(response.into_inner().group.unwrap().priority, 9);
                    break;
                }
                Err(error) => {
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "proxy not ready: {error}: {}",
                        sidecar.logs()
                    );
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        }
        let mut forged = tonic::Request::new(p::PutResourceGroupRequest {
            group: Some(group(12, 25.0)),
        });
        forged.metadata_mut().insert(
            "pd-forwarded-host",
            "http://invalid.example:2379".parse().unwrap(),
        );
        let error = client.modify_resource_group(forged).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        records.push(json!({"rust":is_rust,"stage":"forged PD target rejected","error":format!("{error:?}")}));
        save(&records);
        client
            .modify_resource_group(p::PutResourceGroupRequest {
                group: Some(group(12, 25.0)),
            })
            .await
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let group = client
                .get_resource_group(request.clone())
                .await
                .unwrap()
                .into_inner()
                .group
                .unwrap();
            if group.priority == 12 {
                assert_eq!(group.r_u_settings.unwrap().r_u.unwrap().tokens, 75.0);
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "proxy read bypasses primary metadata watcher"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let listed = client
            .list_resource_groups(p::ListResourceGroupsRequest {
                keyspace_id: scoped(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(
            listed
                .groups
                .iter()
                .any(|group| group.name == "watched" && group.priority == 12)
        );
        records.push(json!({"rust":is_rust,"stage":"proxy live primary read","response":format!("{listed:?}")}));
        save(&records);
        client
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
                    records.push(json!({"rust":is_rust,"stage":"proxy deletion observed","error":format!("{error:?}")}));
                    save(&records);
                    break;
                }
                Ok(_) => {
                    assert!(
                        tokio::time::Instant::now() < deadline,
                        "deleted group remains visible through proxy"
                    );
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        }
        sidecar.stop();
        pd.stop().unwrap();
    }
}
fn proxy_token_request(value: f64) -> p::TokenBucketsRequest {
    p::TokenBucketsRequest {
        requests: vec![p::TokenBucketRequest {
            resource_group_name: "watched".into(),
            keyspace_id: scoped(),
            request: Some(p::token_bucket_request::Request::RuItems(
                p::token_bucket_request::RequestRu {
                    request_r_u: vec![p::RequestUnitItem { r#type: 0, value }],
                },
            )),
            ..Default::default()
        }],
        target_request_period_ms: 5000,
        client_unique_id: 23,
    }
}
async fn proxy_ready(
    client: &mut ResourceManagerClient<tonic::transport::Channel>,
    sidecar: &Sidecar,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if client
            .get_resource_group(p::GetResourceGroupRequest {
                resource_group_name: "watched".into(),
                keyspace_id: scoped(),
                ..Default::default()
            })
            .await
            .is_ok()
        {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "proxy primary not ready: {}",
            sidecar.logs()
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_proxy_token_frames_half_close_and_both_service_deployments() {
    use prost::Message;
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let config = File(std::env::temp_dir().join(format!(
        "pd-sa016-rm-token-proxy-{}.toml",
        std::process::id()
    )));
    std::fs::write(
        &config.0,
        "[micro-service]\nenable-resource-manager-fallback = false\n",
    )
    .unwrap();
    let argument = format!("--config={}", config.0.display());
    let environment = [("PD_SERVICE_MODE".into(), "api".into())];
    let mut expected = None;
    let mut records = Vec::new();
    for (pd_rust, rm_rust) in [(false, false), (true, false), (false, true), (true, true)] {
        let binary = if pd_rust { &rust } else { &go };
        let mut pd = if pd_rust {
            Server::start_rust_with_environment(binary, &[&argument], &environment)
        } else {
            Server::start_with_environment(binary, &[&argument], &environment)
        }
        .unwrap();
        pd.ready().unwrap();
        let mut client = ResourceManagerClient::connect(pd.url.clone())
            .await
            .unwrap();
        let mut resource = group(9, 75.0);
        resource
            .r_u_settings
            .as_mut()
            .unwrap()
            .r_u
            .as_mut()
            .unwrap()
            .settings
            .as_mut()
            .unwrap()
            .burst_limit = -1;
        client
            .add_resource_group(p::PutResourceGroupRequest {
                group: Some(resource),
            })
            .await
            .unwrap();
        let mut sidecar = Sidecar::start(if rm_rust { &rust } else { &go }, pd.metadata_url());
        proxy_ready(&mut client, &sidecar).await;
        let mut first = proxy_token_request(5.0);
        if let Some(p::token_bucket_request::Request::RuItems(items)) =
            &mut first.requests[0].request
        {
            items.request_r_u.push(p::RequestUnitItem {
                r#type: 99,
                value: 999.0,
            });
        }
        let mut second = proxy_token_request(7.0);
        let mut missing = second.requests[0].clone();
        missing.resource_group_name = "missing".into();
        second.requests.push(missing);
        let input = [first, second, p::TokenBucketsRequest::default()];
        let (send, receive) = tokio::sync::mpsc::channel(4);
        for request in &input {
            send.send(request.clone()).await.unwrap();
        }
        drop(send);
        let mut stream = client
            .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(receive))
            .await
            .unwrap()
            .into_inner();
        let mut frames = Vec::new();
        while let Some(response) = tokio::time::timeout(Duration::from_secs(3), stream.message())
            .await
            .expect("half-close must drain responses and EOF")
            .unwrap()
        {
            records.push(json!({"pd_rust":pd_rust,"rm_rust":rm_rust,"stage":"proxy token response","response":format!("{response:?}")}));
            save(&records);
            frames.push(response);
        }
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].responses[0].granted_r_u_tokens.len(), 2);
        assert_eq!(
            frames[0].responses[0].granted_r_u_tokens[0]
                .granted_tokens
                .as_ref()
                .unwrap()
                .tokens,
            5.0
        );
        assert_eq!(
            frames[0].responses[0].granted_r_u_tokens[1]
                .granted_tokens
                .as_ref()
                .unwrap()
                .tokens,
            5.0
        );
        assert_eq!(frames[1].responses.len(), 1);
        assert_eq!(
            frames[1].responses[0].granted_r_u_tokens[0]
                .granted_tokens
                .as_ref()
                .unwrap()
                .tokens,
            7.0
        );
        assert!(frames[2].responses.is_empty());
        let encoded = frames
            .iter()
            .map(Message::encode_to_vec)
            .collect::<Vec<_>>();
        if let Some(expected) = &expected {
            assert_eq!(
                &encoded, expected,
                "forwarding must preserve exact frame order and protobuf values"
            );
        } else {
            expected = Some(encoded);
        }
        records.push(json!({"pd_rust":pd_rust,"rm_rust":rm_rust,"stage":"half-close drained exact responses and EOF","requests":input.iter().map(|r|format!("{r:?}")).collect::<Vec<_>>()}));
        save(&records);
        sidecar.stop();
        pd.stop().unwrap();
    }
}
async fn rm_disconnect_snapshot(store: &EtcdMetaStore) -> Value {
    let mut snapshot = serde_json::Map::new();
    for (name, key) in [
        (
            "setting",
            astersql_pd_keypath::keyspace_resource_group_setting_path(7, "watched"),
        ),
        (
            "state",
            astersql_pd_keypath::keyspace_resource_group_state_path(7, "watched"),
        ),
    ] {
        let read = store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        snapshot.insert(
            name.into(),
            json!(read.entries.iter().map(|entry| {
            json!({"key":entry.key,"value":entry.value,"create_revision":entry.create_revision,
                   "mod_revision":entry.mod_revision,"lease":entry.lease})
        }).collect::<Vec<_>>()),
        );
    }
    Value::Object(snapshot)
}

async fn rm_disconnect_case(
    go: &Path,
    binary: &Path,
    is_rust: bool,
    wait_for_go_close: bool,
    records: &mut Vec<Value>,
) -> Result<p::TokenBucketsResponse, String> {
    use prost::Message;

    let config = File(std::env::temp_dir().join(format!(
        "pd-sa016-rm-cancel-proxy-{}-{}.toml",
        std::process::id(),
        is_rust
    )));
    std::fs::write(
        &config.0,
        "[micro-service]\nenable-resource-manager-fallback = false\n",
    )
    .unwrap();
    let argument = format!("--config={}", config.0.display());
    let environment = [("PD_SERVICE_MODE".into(), "api".into())];
    let args = [argument.as_str(), "--log-level=debug"];
    let mut pd = if is_rust {
        Server::start_rust_with_environment(binary, &args, &environment)
    } else {
        Server::start_with_environment(binary, &args, &environment)
    }
    .unwrap();
    pd.ready().unwrap();
    let mut client = ResourceManagerClient::connect(pd.url.clone())
        .await
        .unwrap();
    let mut resource = group(9, 75.0);
    resource
        .r_u_settings
        .as_mut()
        .unwrap()
        .r_u
        .as_mut()
        .unwrap()
        .settings
        .as_mut()
        .unwrap()
        .burst_limit = -1;
    let put = p::PutResourceGroupRequest {
        group: Some(resource),
    };
    let added = client
        .add_resource_group(put.clone())
        .await
        .unwrap()
        .into_inner();
    let store = EtcdMetaStore::connect(&[pd.metadata_url().into()], Duration::from_secs(3))
        .await
        .unwrap();
    let mut sidecar = Sidecar::start(go, pd.metadata_url());
    proxy_ready(&mut client, &sidecar).await;

    let (send_a, receive_a) = tokio::sync::mpsc::channel(1);
    let request_a = proxy_token_request(5.0);
    send_a.send(request_a.clone()).await.unwrap();
    let front = astersql_pd_mcs_server::DelegateConnections::new(
        tokio::runtime::Handle::current(),
        &tokio_util::sync::CancellationToken::new(),
        None,
    );
    let mut client_a = ResourceManagerClient::new(front.get(&pd.url).await.unwrap());
    let mut a = client_a
        .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(receive_a))
        .await
        .unwrap()
        .into_inner();
    let first_a = a.message().await.unwrap().unwrap();

    let (send_b, receive_b) = tokio::sync::mpsc::channel(1);
    let mut request_b = proxy_token_request(7.0);
    request_b.client_unique_id = 24;
    send_b.send(request_b.clone()).await.unwrap();
    let mut b = client
        .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(receive_b))
        .await
        .unwrap()
        .into_inner();
    let first_b = b.message().await.unwrap().unwrap();
    let before = rm_disconnect_snapshot(&store).await;

    let close_line = |line: &&str| {
        line.contains("close delegate client connection") && line.contains(&sidecar.url)
    };
    let previous_closures = pd.logs().lines().filter(close_line).count();
    let delegate_closed = || {
        pd.logs()
            .lines()
            .filter(close_line)
            .nth(previous_closures)
            .map(str::to_owned)
    };
    front.shutdown();
    let first_end = tokio::time::timeout(Duration::from_secs(1), a.message())
        .await
        .unwrap();
    assert!(
        !matches!(first_end, Ok(Some(_))),
        "frontend connection shutdown must terminate the first RPC"
    );
    if wait_for_go_close && !is_rust {
        tokio::time::timeout(Duration::from_secs(3), async {
            while delegate_closed().is_none() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Go did not log closure of the delegate used by both streams");
    }

    // Both senders stay alive; closing the owned frontend TCP transport is cancellation.
    let mut next = proxy_token_request(9.0);
    next.client_unique_id = 24;
    send_b.send(next.clone()).await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(3), b.message()).await;
    let after = rm_disconnect_snapshot(&store).await;
    let response_bytes = response
        .as_ref()
        .ok()
        .and_then(|r| r.as_ref().ok())
        .and_then(|r| r.as_ref())
        .map(Message::encode_to_vec);
    records.push(json!({
        "rust":is_rust,"wait_for_go_close":wait_for_go_close,"pd":pd.url,
        "metadata":pd.metadata_url(),"sidecar":sidecar.url,
        "put":put.encode_to_vec(),"added":added.encode_to_vec(),
        "requests":[request_a.encode_to_vec(),request_b.encode_to_vec(),next.encode_to_vec()],
        "initial_responses":[first_a.encode_to_vec(),first_b.encode_to_vec()],
        "first_sender_closed":send_a.is_closed(),"first_stream":format!("{first_end:?}"),
        "other_stream":format!("{response:?}"),"other_response":response_bytes,
        "previous_delegate_closures":previous_closures,"delegate_closed_before_cleanup":delegate_closed(),
        "etcd_before":before,"etcd_after":after,"pd_log_before_cleanup":pd.logs(),
        "sidecar_log_before_cleanup":sidecar.logs(),
    }));
    save(records);
    assert_eq!(before["setting"].as_array().unwrap().len(), 1);
    assert_eq!(
        before["setting"], after["setting"],
        "RM settings changed after disconnect"
    );

    drop(send_a);
    drop(send_b);
    drop(b);
    proxy_ready(&mut client, &sidecar).await;
    sidecar.stop();
    pd.stop().unwrap();
    records.last_mut().unwrap()["graceful_cleanup"] = json!(true);
    save(records);
    response
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "unexpected EOF".into())
}

fn assert_surviving_rm_stream(response: Result<p::TokenBucketsResponse, String>) {
    let response = response.expect("other stream must keep responding after frontend disconnect");
    assert_eq!(
        response.responses[0].granted_r_u_tokens[0]
            .granted_tokens
            .as_ref()
            .unwrap()
            .tokens,
        9.0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_proxy_frontend_disconnect_rust_branch() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    assert_surviving_rm_stream(rm_disconnect_case(&go, &rust, true, false, &mut Vec::new()).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_proxy_frontend_disconnect_preserves_other_stream() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let wait_for_go_close = std::env::var_os("PD_SA019_RM_WAIT_FOR_GO_CLOSE").is_some();
    let mut records = Vec::new();
    let rust_response = rm_disconnect_case(&go, &rust, true, false, &mut records).await;
    let go_response = rm_disconnect_case(&go, &go, false, wait_for_go_close, &mut records).await;
    assert_surviving_rm_stream(rust_response);
    assert_surviving_rm_stream(go_response);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_proxy_upstream_loss_then_next_send_ends_without_replay() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let config = File(std::env::temp_dir().join(format!(
        "pd-sa016-rm-loss-proxy-{}.toml",
        std::process::id()
    )));
    std::fs::write(
        &config.0,
        "[micro-service]\nenable-resource-manager-fallback = false\n",
    )
    .unwrap();
    let argument = format!("--config={}", config.0.display());
    let environment = [("PD_SERVICE_MODE".into(), "api".into())];
    let mut records = Vec::new();
    let mut expected = None;
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut pd = if is_rust {
            Server::start_rust_with_environment(binary, &[&argument], &environment)
        } else {
            Server::start_with_environment(binary, &[&argument], &environment)
        }
        .unwrap();
        pd.ready().unwrap();
        let mut client = ResourceManagerClient::connect(pd.url.clone())
            .await
            .unwrap();
        let mut resource = group(9, 75.0);
        resource
            .r_u_settings
            .as_mut()
            .unwrap()
            .r_u
            .as_mut()
            .unwrap()
            .settings
            .as_mut()
            .unwrap()
            .burst_limit = -1;
        client
            .add_resource_group(p::PutResourceGroupRequest {
                group: Some(resource),
            })
            .await
            .unwrap();
        let mut sidecar = Sidecar::start(&go, pd.metadata_url());
        proxy_ready(&mut client, &sidecar).await;
        let (send, receive) = tokio::sync::mpsc::channel(1);
        send.send(proxy_token_request(5.0)).await.unwrap();
        let mut stream = client
            .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(receive))
            .await
            .unwrap()
            .into_inner();
        assert!(stream.message().await.unwrap().is_some());
        sidecar.stop();
        let sent = send.send(proxy_token_request(9.0)).await;
        let ended = tokio::time::timeout(Duration::from_secs(3), stream.message()).await;
        records.push(json!({"rust":is_rust,"stage":"upstream stopped then next input","send":format!("{sent:?}"),"result":format!("{ended:?}")}));
        save(&records);
        let status = match ended {
            Ok(Err(error)) => Some(error.code()),
            Ok(Ok(None)) => None,
            other => {
                panic!("proxy must terminate without replaying after upstream exit: {other:?}")
            }
        };
        if let Some(expected) = expected {
            assert_eq!(
                status, expected,
                "upstream transport error code must match Go proxy"
            );
        } else {
            expected = Some(status);
        }
        drop(send);
        drop(stream);
        sidecar = Sidecar::start(&go, pd.metadata_url());
        proxy_ready(&mut client, &sidecar).await;
        let (send, receive) = tokio::sync::mpsc::channel(1);
        send.send(proxy_token_request(7.0)).await.unwrap();
        drop(send);
        let mut stream = client
            .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(receive))
            .await
            .unwrap()
            .into_inner();
        let response = stream.message().await.unwrap().unwrap();
        assert_eq!(
            response.responses[0].granted_r_u_tokens[0]
                .granted_tokens
                .as_ref()
                .unwrap()
                .tokens,
            7.0
        );
        assert!(stream.message().await.unwrap().is_none());
        records.push(json!({"rust":is_rust,"stage":"fresh primary new stream","response":format!("{response:?}")}));
        save(&records);
        sidecar.stop();
        pd.stop().unwrap();
    }
}

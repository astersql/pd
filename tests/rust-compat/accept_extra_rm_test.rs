// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_keypath::{
    keyspace_resource_group_setting_path, keyspace_resource_group_state_path,
};
use astersql_pd_proto::resource_manager::{
    self as p, resource_manager_client::ResourceManagerClient,
};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use astersql_pd_storage_kv::EtcdMetaStore;
use prost::Message;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};
fn keyspace(id: u32) -> Option<p::KeyspaceIdValue> {
    Some(p::KeyspaceIdValue {
        keyspace: Some(p::keyspace_id_value::Keyspace::Value(id)),
    })
}
fn group(name: &str, id: u32) -> p::ResourceGroup {
    p::ResourceGroup {
        name: name.into(),
        mode: 1,
        priority: 9,
        keyspace_id: keyspace(id),
        r_u_settings: Some(p::GroupRequestUnitSettings {
            r_u: Some(p::TokenBucket {
                settings: Some(p::TokenLimitSettings {
                    fill_rate: 500,
                    burst_limit: 2000,
                    max_tokens: 40.0,
                }),
                tokens: 75.0,
            }),
        }),
        background_settings: Some(p::BackgroundSettings {
            job_types: vec!["backup".into()],
            utilization_limit: 30,
        }),
        runaway_settings: Some(p::RunawaySettings {
            action: 2,
            switch_group_name: "default".into(),
            rule: Some(p::RunawayRule {
                exec_elapsed_time_ms: 100,
                processed_keys: 20,
                request_unit: 5,
            }),
            watch: Some(p::RunawayWatch {
                lasting_duration_ms: 80,
                r#type: 1,
            }),
        }),
        ru_stats: Some(p::Consumption {
            r_r_u: 1.5,
            read_cross_az_traffic_bytes: 3,
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn outcome<T: std::fmt::Debug>(result: Result<tonic::Response<T>, tonic::Status>) -> String {
    match result {
        Ok(response) => format!("{:?}", response.into_inner()),
        Err(error) => format!("{:?}: {}", error.code(), error.message()),
    }
}
fn save(records: &[Value]) {
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap());
    let kind = if records.first().and_then(|v| v.get("request")).is_some() {
        "metadata"
    } else {
        "tokens"
    };
    let path = path.with_file_name(format!(
        "{}-{kind}.json",
        path.file_name().unwrap().to_string_lossy()
    ));
    std::fs::write(path, serde_json::to_vec(records).unwrap()).unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_metadata() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").unwrap(), "dual");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    let mut expected = None;
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let mut client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        let response = client
            .list_resource_groups(p::ListResourceGroupsRequest::default())
            .await;
        records.push(json!({"rust":is_rust,"request":"ListResourceGroups default/null keyspace","response":format!("{response:?}")}));
        save(&records);
        let response = response
            .expect("classic fallback must expose the canonical RM manager")
            .into_inner();
        assert_eq!(response.groups.len(), 1);
        let default_group = &response.groups[0];
        assert_eq!(default_group.name, "default");
        assert_eq!(default_group.priority, 8);
        assert_eq!(default_group.mode, p::GroupMode::RuMode as i32);
        assert!(default_group.ru_stats.is_none());
        let mut observations = Vec::new();
        macro_rules! call {($name:expr,$response:expr)=>{{
            let response=outcome($response);
            records.push(json!({"rust":is_rust,"scenario":$name,"response":response}));save(&records);
            observations.push(($name.to_string(),response));
        }}}
        call!(
            "null with stats",
            client
                .list_resource_groups(p::ListResourceGroupsRequest {
                    with_ru_stats: true,
                    ..Default::default()
                })
                .await
        );
        call!(
            "unknown keyspace group",
            client
                .get_resource_group(p::GetResourceGroupRequest {
                    keyspace_id: keyspace(701),
                    resource_group_name: "missing".into(),
                    ..Default::default()
                })
                .await
        );
        call!(
            "new keyspace default",
            client
                .get_resource_group(p::GetResourceGroupRequest {
                    keyspace_id: keyspace(7),
                    resource_group_name: "default".into(),
                    ..Default::default()
                })
                .await
        );
        call!(
            "empty wrapper is keyspace zero",
            client
                .list_resource_groups(p::ListResourceGroupsRequest {
                    keyspace_id: Some(p::KeyspaceIdValue::default()),
                    ..Default::default()
                })
                .await
        );
        call!(
            "add",
            client
                .add_resource_group(p::PutResourceGroupRequest {
                    group: Some(group("zeta", 7))
                })
                .await
        );
        call!(
            "get stats",
            client
                .get_resource_group(p::GetResourceGroupRequest {
                    keyspace_id: keyspace(7),
                    resource_group_name: "zeta".into(),
                    with_ru_stats: true
                })
                .await
        );
        call!(
            "add alphabetically earlier",
            client
                .add_resource_group(p::PutResourceGroupRequest {
                    group: Some(group("alpha", 7))
                })
                .await
        );
        call!(
            "sorted list",
            client
                .list_resource_groups(p::ListResourceGroupsRequest {
                    keyspace_id: keyspace(7),
                    ..Default::default()
                })
                .await
        );
        let mut update = group("zeta", 7);
        update.priority = 16;
        update
            .r_u_settings
            .as_mut()
            .unwrap()
            .r_u
            .as_mut()
            .unwrap()
            .tokens = 25.0;
        call!(
            "modify token delta",
            client
                .modify_resource_group(p::PutResourceGroupRequest {
                    group: Some(update)
                })
                .await
        );
        call!(
            "get after modify",
            client
                .get_resource_group(p::GetResourceGroupRequest {
                    keyspace_id: keyspace(7),
                    resource_group_name: "zeta".into(),
                    with_ru_stats: true
                })
                .await
        );
        let mut invalid = group("zeta", 7);
        invalid.priority = 4;
        invalid.r_u_settings = None;
        call!(
            "modify missing RU settings",
            client
                .modify_resource_group(p::PutResourceGroupRequest {
                    group: Some(invalid)
                })
                .await
        );
        call!(
            "partial in memory patch",
            client
                .get_resource_group(p::GetResourceGroupRequest {
                    keyspace_id: keyspace(7),
                    resource_group_name: "zeta".into(),
                    with_ru_stats: false
                })
                .await
        );
        let mut invalid = group("zeta", 7);
        invalid.priority = 17;
        call!(
            "add invalid priority",
            client
                .add_resource_group(p::PutResourceGroupRequest {
                    group: Some(invalid.clone())
                })
                .await
        );
        call!(
            "modify invalid priority",
            client
                .modify_resource_group(p::PutResourceGroupRequest {
                    group: Some(invalid)
                })
                .await
        );
        call!(
            "add long UTF8 name",
            client
                .add_resource_group(p::PutResourceGroupRequest {
                    group: Some(group(&"组".repeat(11), 7))
                })
                .await
        );
        call!(
            "add empty name",
            client
                .add_resource_group(p::PutResourceGroupRequest {
                    group: Some(group("", 7))
                })
                .await
        );
        call!(
            "modify missing group",
            client
                .modify_resource_group(p::PutResourceGroupRequest {
                    group: Some(group("absent", 7))
                })
                .await
        );
        call!(
            "delete reserved",
            client
                .delete_resource_group(p::DeleteResourceGroupRequest {
                    resource_group_name: "default".into(),
                    keyspace_id: keyspace(7)
                })
                .await
        );
        call!(
            "delete missing keyspace",
            client
                .delete_resource_group(p::DeleteResourceGroupRequest {
                    resource_group_name: "default".into(),
                    keyspace_id: keyspace(999)
                })
                .await
        );
        call!(
            "add idempotent replacement",
            client
                .add_resource_group(p::PutResourceGroupRequest {
                    group: Some(group("zeta", 7))
                })
                .await
        );
        call!(
            "delete group",
            client
                .delete_resource_group(p::DeleteResourceGroupRequest {
                    resource_group_name: "alpha".into(),
                    keyspace_id: keyspace(7)
                })
                .await
        );
        call!(
            "delete missing group",
            client
                .delete_resource_group(p::DeleteResourceGroupRequest {
                    resource_group_name: "alpha".into(),
                    keyspace_id: keyspace(7)
                })
                .await
        );
        // Independent backend reads establish actual wire persistence and deletion.
        let store = EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        for name in ["zeta", "alpha", "default"] {
            let key = keyspace_resource_group_setting_path(7, name);
            let snapshot = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            let persisted = snapshot
                .entries
                .first()
                .map(|e| p::ResourceGroup::decode(e.value.as_slice()).unwrap());
            assert_eq!(persisted.is_none(), name == "alpha");
            let key = keyspace_resource_group_state_path(7, name);
            let snapshot = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            let state = snapshot
                .entries
                .first()
                .map(|e| serde_json::from_slice::<Value>(&e.value).unwrap());
            let observation = format!("{persisted:?} / {state:?}");
            records.push(json!({"rust":is_rust,"scenario":format!("persisted {name}"),"response":observation}));
            save(&records);
            observations.push((format!("persisted {name}"), observation));
        }
        if is_rust {
            server.restart_rust(binary).unwrap();
        } else {
            server.restart_with_args(binary, &[]).unwrap();
        }
        server.ready().unwrap();
        client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        call!(
            "WAL restart restores persisted group",
            client
                .get_resource_group(p::GetResourceGroupRequest {
                    keyspace_id: keyspace(7),
                    resource_group_name: "zeta".into(),
                    with_ru_stats: true
                })
                .await
        );
        if let Some(expected) = &expected {
            assert_eq!(
                &observations, expected,
                "RM metadata must match the pinned Go process"
            );
        } else {
            expected = Some(observations);
        }
        server.stop().unwrap();
    }
}

fn token_request(name: &str, value: f64, client: u64) -> p::TokenBucketsRequest {
    p::TokenBucketsRequest {
        requests: vec![p::TokenBucketRequest {
            resource_group_name: name.into(),
            request: Some(p::token_bucket_request::Request::RuItems(
                p::token_bucket_request::RequestRu {
                    request_r_u: vec![p::RequestUnitItem { r#type: 0, value }],
                },
            )),
            ..Default::default()
        }],
        target_request_period_ms: 5000,
        client_unique_id: client,
    }
}
async fn exchange(
    send: &tokio::sync::mpsc::Sender<p::TokenBucketsRequest>,
    stream: &mut tonic::Streaming<p::TokenBucketsResponse>,
    request: p::TokenBucketsRequest,
) -> p::TokenBucketsResponse {
    send.send(request).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_tokens() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").unwrap(), "dual");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    let mut expected = None;
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let mut client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        for name in ["rate", "moderated", "missing-settings", "finite"] {
            let mut group = group(name, u32::MAX);
            group.ru_stats = None;
            let bucket = group.r_u_settings.as_mut().unwrap().r_u.as_mut().unwrap();
            bucket.tokens = 200000.0;
            bucket.settings.as_mut().unwrap().fill_rate = 1000;
            bucket.settings.as_mut().unwrap().burst_limit = match name {
                "rate" => 0,
                "moderated" => -2,
                _ => 20000000,
            };
            if name == "missing-settings" {
                group.r_u_settings = None;
            }
            client
                .add_resource_group(p::PutResourceGroupRequest { group: Some(group) })
                .await
                .unwrap();
        }
        let (send, recv) = tokio::sync::mpsc::channel(4);
        send.send(token_request("default", 1500.0, 12))
            .await
            .unwrap();
        let opened = client
            .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(recv))
            .await;
        records.push(json!({"rust":is_rust,"scenario":"default unlimited token stream open","response":format!("{opened:?}")}));
        save(&records);
        let mut stream = opened
            .expect("canonical RM token stream must serve")
            .into_inner();
        let response = stream.message().await.unwrap().unwrap();
        let grant = &response.responses[0].granted_r_u_tokens[0];
        assert_eq!(grant.granted_tokens.as_ref().unwrap().tokens, 1500.0);
        assert_eq!(grant.trickle_time_ms, 0);
        let mut observations = vec![(
            "default unlimited grant".to_string(),
            format!("{response:?}"),
        )];
        macro_rules! record {($label:expr,$request:expr)=>{{
            let request=$request;
            let input=format!("{request:?}");
            let response=exchange(&send,&mut stream,request).await;
            records.push(json!({"rust":is_rust,"scenario":$label,"input":input,"response":format!("{response:?}")}));save(&records);
            observations.push(($label.to_string(),format!("{response:?}")));response
        }}}
        let result = record!("missing groups skip", token_request("absent", 20.0, 12));
        assert!(result.responses.is_empty());
        let result = record!(
            "missing settings omit grant",
            token_request("missing-settings", 20.0, 12)
        );
        assert!(result.responses[0].granted_r_u_tokens.is_empty());
        let result = record!(
            "rate controlled first loan",
            token_request("rate", 15000.0, 12)
        );
        assert_eq!(
            result.responses[0].granted_r_u_tokens[0]
                .granted_tokens
                .as_ref()
                .unwrap()
                .tokens,
            10000.0
        );
        let result = record!("second client share", token_request("rate", 10000.0, 13));
        assert_eq!(
            result.responses[0].granted_r_u_tokens[0]
                .granted_tokens
                .as_ref()
                .unwrap()
                .tokens,
            5000.0
        );
        record!("remove client slot", token_request("rate", 0.0, 13));
        record!("moderated capacity", token_request("moderated", 2000.0, 12));
        let result = record!(
            "finite existing balance",
            token_request("finite", 190000.0, 12)
        );
        assert_eq!(result.responses[0].granted_r_u_tokens[0].trickle_time_ms, 0);
        let result = record!("nonfinite withheld", token_request("default", f64::NAN, 12));
        assert!(result.responses[0].granted_r_u_tokens.is_empty());
        record!(
            "recovery after nonfinite",
            token_request("default", 2000.0, 12)
        );
        let mut request = token_request("default", 4.0, 12);
        if let Some(p::token_bucket_request::Request::RuItems(items)) =
            &mut request.requests[0].request
        {
            items.request_r_u.push(p::RequestUnitItem {
                r#type: 99,
                value: 100.0,
            });
        }
        let result = record!("unknown RU enum retains previous grant", request);
        assert_eq!(result.responses[0].granted_r_u_tokens.len(), 2);
        let mut request = token_request("default", 4.0, 12);
        request.requests[0].is_background = true;
        request.requests[0].consumption_since_last_request = Some(p::Consumption {
            r_r_u: 2.0,
            ..Default::default()
        });
        let result = record!("background records consumption without grant", request);
        assert!(result.responses.is_empty());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let stats = client
                .get_resource_group(p::GetResourceGroupRequest {
                    resource_group_name: "default".into(),
                    with_ru_stats: true,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner()
                .group
                .unwrap()
                .ru_stats
                .unwrap();
            if stats.r_r_u == 2.0 {
                records.push(json!({"rust":is_rust,"scenario":"background accumulated consumption","response":format!("{stats:?}")}));
                save(&records);
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "consumption not applied: {stats:?}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        drop(send);
        assert!(stream.message().await.unwrap().is_none());
        let (send, recv) = tokio::sync::mpsc::channel(1);
        let mut request = token_request("default", 1.0, 12);
        request.requests[0].is_background = true;
        request.requests[0].is_tiflash = true;
        send.send(request).await.unwrap();
        let result = client
            .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(recv))
            .await
            .unwrap_err();
        assert_eq!(result.code(), tonic::Code::Unknown);
        assert_eq!(
            result.message(),
            "background and tiflash cannot be true at the same time"
        );
        observations.push((
            "background plus tiflash".into(),
            format!("{:?}: {}", result.code(), result.message()),
        ));
        drop(send);
        let (send, recv) = tokio::sync::mpsc::channel(1);
        let mut request = tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(recv));
        request.set_timeout(Duration::from_millis(300));
        let started = tokio::time::Instant::now();
        let result = client.acquire_token_buckets(request).await.unwrap_err();
        let elapsed = started.elapsed();
        records.push(json!({"rust":is_rust,"scenario":"idle request deadline raw client result","code":format!("{:?}",result.code()),"message":result.message(),"elapsed_ms":elapsed.as_millis()}));
        save(&records);
        assert_eq!(result.code(), tonic::Code::Cancelled);
        assert!(
            elapsed >= Duration::from_millis(250) && elapsed < Duration::from_secs(2),
            "idle deadline did not cancel at the request budget: {elapsed:?}"
        );
        // These texts originate in the client's competing deadline and HTTP/2 reset paths.
        // Preserve both raw results above; the transport code and bounded cancellation are contractual.
        observations.push((
            "idle request deadline".into(),
            format!("{:?}", result.code()),
        ));
        drop(send);
        records.push(json!({"rust":is_rust,"scenario":"complete stream observations","response":observations}));
        save(&records);
        if let Some(expected) = &expected {
            assert_eq!(
                &observations, expected,
                "RM stream results must match pinned Go"
            );
        } else {
            expected = Some(observations);
        }
        server.stop().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_invalid_floats() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut observations = Vec::new();
    let mut expected = None;
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let mut client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        let store = EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        let mut actual = Vec::new();
        for (name, value) in [
            ("badnan", f64::NAN),
            ("badposinf", f64::INFINITY),
            ("badneginf", f64::NEG_INFINITY),
        ] {
            let mut group = group(name, 7);
            group
                .r_u_settings
                .as_mut()
                .unwrap()
                .r_u
                .as_mut()
                .unwrap()
                .tokens = value;
            let result = outcome(
                client
                    .add_resource_group(p::PutResourceGroupRequest { group: Some(group) })
                    .await,
            );
            let settings = store
                .read(Read::range(
                    keyspace_resource_group_setting_path(7, name).as_bytes(),
                    b"",
                    Consistency::Linearizable,
                ))
                .await
                .unwrap();
            let state = store
                .read(Read::range(
                    keyspace_resource_group_state_path(7, name).as_bytes(),
                    b"",
                    Consistency::Linearizable,
                ))
                .await
                .unwrap();
            assert_eq!(
                settings.entries.len(),
                1,
                "Go persists settings before rejecting JSON state"
            );
            assert!(state.entries.is_empty());
            let persisted = p::ResourceGroup::decode(settings.entries[0].value.as_slice())
                .unwrap()
                .r_u_settings
                .unwrap()
                .r_u
                .unwrap()
                .tokens;
            assert!(if value.is_nan() {
                persisted.is_nan()
            } else {
                persisted == value
            });
            let read = outcome(
                client
                    .get_resource_group(p::GetResourceGroupRequest {
                        resource_group_name: name.into(),
                        keyspace_id: keyspace(7),
                        ..Default::default()
                    })
                    .await,
            );
            observations.push(json!({"rust":is_rust,"scenario":format!("nonfinite {name}"),"add":result,"get":read,"physical_settings":settings.entries.len(),"physical_states":state.entries.len()}));
            let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap());
            let path = path.with_file_name(format!(
                "{}-invalid-floats.json",
                path.file_name().unwrap().to_string_lossy()
            ));
            std::fs::write(path, serde_json::to_vec(&observations).unwrap()).unwrap();
            actual.push((result, read));
        }
        server.stop().unwrap();
        if let Some(expected) = &expected {
            assert_eq!(&actual, expected);
        } else {
            expected = Some(actual);
        }
    }
}

struct ConfigFile(PathBuf);
impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_controller_initial_values() {
    let path = std::env::temp_dir().join(format!(
        "pd-sa016-rm-config-{}-{}.toml",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let config = ConfigFile(path);
    std::fs::write(
        &config.0,
        "[controller.request-unit]\nread-base-cost = 2.5\n",
    )
    .unwrap();
    let arg = format!("--config={}", config.0.display());
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut expected = None;
    let mut records = Vec::new();
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust_with_args(binary, &[&arg])
        } else {
            Server::start_with_args(binary, &[&arg])
        }
        .unwrap();
        server.ready().unwrap();
        let store = EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        let mut actual = Vec::new();
        for (stage,input) in [
            ("initial host config",None),
            ("stored partial controller overlay",Some(br#"{"request-unit":{"read-base-cost":3}}"#.as_slice())),
            ("stored incompatible field retains other fields",Some(br#"{"ltb-max-wait-duration":"7s","request-unit":{"read-base-cost":"invalid","write-base-cost":6}}"#.as_slice())),
            ("stored null scalar and value struct",Some(br#"{"request-unit":null,"ltb-max-wait-duration":null,"ru-version-policy":null}"#.as_slice())),
            ("stored root null",Some(b"null".as_slice())),
        ] {
            let key = astersql_pd_keypath::controller_config_path();
            if let Some(input)=input {
                store
                    .transact(
                        vec![],
                        vec![astersql_pd_storage_domain::Mutation::put(
                            key.as_bytes(),
                            input,
                        )],
                    )
                    .await
                    .unwrap();
                if is_rust {
                    let endpoint = format!("--etcd-endpoints={}", server.metadata_url());
                    server
                        .restart_with_args(binary, &[&endpoint, &arg])
                        .unwrap();
                } else {
                    server.restart_with_args(binary, &[&arg]).unwrap();
                }
                server.ready().unwrap();
            }
            let data = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            let value: Value = serde_json::from_slice(&data.entries[0].value).unwrap();
            records.push(json!({"rust":is_rust,"scenario":stage,"controller":value}));
            let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap());
            let path=path.with_file_name(format!("{}-controller.json",path.file_name().unwrap().to_string_lossy()));
            std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
            actual.push(value);
        }
        server.stop().unwrap();
        if let Some(expected) = &expected {
            assert_eq!(
                &actual, expected,
                "controller persistence must inherit configured defaults before applying stored fields"
            );
        } else {
            expected = Some(actual);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_restore_state_boundaries() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut expected = None;
    let mut records = Vec::new();
    let states=[
        ("null-fields",br#"{"r_u":{"tokens":null,"initialized":null},"ru_consumption":{"r_r_u":null,"read_cross_az_traffic_bytes":null}}"#.as_slice()),
        ("bad-time",br#"{"r_u":{"tokens":32,"last_update":"not-a-time","initialized":true},"ru_consumption":{"r_r_u":10}}"#.as_slice()),
        ("no-ru",br#"{"ru_consumption":{"r_r_u":4}}"#.as_slice()),
        ("null-state",b"null".as_slice()),
        ("valid",br#"{"r_u":{"tokens":32,"last_update":"2026-11-06T00:00:00Z","initialized":true},"ru_consumption":{"r_r_u":4}}"#.as_slice()),
    ];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let mut client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        let store = EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        for (name, bytes) in states {
            client
                .add_resource_group(p::PutResourceGroupRequest {
                    group: Some(group(name, 7)),
                })
                .await
                .unwrap();
            store
                .transact(
                    vec![],
                    vec![astersql_pd_storage_domain::Mutation::put(
                        keyspace_resource_group_state_path(7, name).as_bytes(),
                        bytes,
                    )],
                )
                .await
                .unwrap();
        }
        if is_rust {
            server.restart_rust(binary).unwrap();
        } else {
            server.restart_with_args(binary, &[]).unwrap();
        }
        server.ready().unwrap();
        client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        let mut actual = Vec::new();
        for (name, input) in states {
            let response = outcome(
                client
                    .get_resource_group(p::GetResourceGroupRequest {
                        resource_group_name: name.into(),
                        keyspace_id: keyspace(7),
                        with_ru_stats: true,
                    })
                    .await,
            );
            records.push(json!({"rust":is_rust,"scenario":name,"input":std::str::from_utf8(input).unwrap(),"response":response}));
            let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap());
            let path = path.with_file_name(format!(
                "{}-state.json",
                path.file_name().unwrap().to_string_lossy()
            ));
            std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
            actual.push(response);
        }
        server.stop().unwrap();
        if let Some(expected) = &expected {
            assert_eq!(&actual, expected);
        } else {
            expected = Some(actual);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_quota() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let mut client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        let mut other = group("other", u32::MAX);
        other.ru_stats = None;
        let bucket = other.r_u_settings.as_mut().unwrap().r_u.as_mut().unwrap();
        bucket.tokens = 0.0;
        bucket.settings.as_mut().unwrap().fill_rate = i32::MAX as u64;
        bucket.settings.as_mut().unwrap().burst_limit = -1;
        client
            .add_resource_group(p::PutResourceGroupRequest { group: Some(other) })
            .await
            .unwrap();
        let store = EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        let key = astersql_pd_keypath::keyspace_service_limit_path(u32::MAX);
        let written = store
            .transact(
                vec![],
                vec![astersql_pd_storage_domain::Mutation::put(
                    key.as_bytes(),
                    b"10",
                )],
            )
            .await
            .unwrap();
        if is_rust {
            server.restart_rust(binary).unwrap();
        } else {
            server.restart_with_args(binary, &[]).unwrap();
        }
        server.ready().unwrap();
        client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        // Both independent process clocks have more than five seconds to fill the
        // shared 5*rate service bucket, making the first 50-token grant exact.
        tokio::time::sleep(Duration::from_millis(5500)).await;
        let mut request = token_request("default", 100000.0, 41);
        request
            .requests
            .push(token_request("other", 100000.0, 41).requests.remove(0));
        let mut unscoped = token_request("default", 100000.0, 41).requests.remove(0);
        unscoped.keyspace_id = keyspace(7);
        request.requests.push(unscoped);
        let (send, recv) = tokio::sync::mpsc::channel(1);
        send.send(request.clone()).await.unwrap();
        let started = tokio::time::Instant::now();
        let mut stream = client
            .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(recv))
            .await
            .unwrap()
            .into_inner();
        let mut total = 0.0;
        for batch in 0..2 {
            if batch == 1 {
                tokio::time::sleep(Duration::from_millis(200)).await;
                send.send(request.clone()).await.unwrap();
            }
            let response = tokio::time::timeout(Duration::from_secs(3), stream.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let elapsed = started.elapsed();
            records.push(json!({"rust":is_rust,"scenario":"keyspace service limit shared by two groups","batch":batch,"request":format!("{request:?}"),"response":format!("{response:?}"),"elapsed_ms":elapsed.as_millis()}));
            let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap());
            let path = path.with_file_name(format!(
                "{}-quota.json",
                path.file_name().unwrap().to_string_lossy()
            ));
            std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
            assert_eq!(response.responses.len(), 3);
            for item in &response.responses[..2] {
                let grant = &item.granted_r_u_tokens[0];
                let tokens = grant.granted_tokens.as_ref().unwrap();
                assert_eq!(
                    tokens.settings.as_ref().unwrap().burst_limit,
                    10,
                    "stored service limits must invalidate unlimited client buckets"
                );
                assert!((0.0..=50.0).contains(&tokens.tokens));
                assert_eq!(
                    grant.trickle_time_ms,
                    (tokens.tokens * 100.0).round() as i64
                );
                total += tokens.tokens;
            }
            if batch == 0 {
                assert_eq!(
                    response.responses[0].granted_r_u_tokens[0]
                        .granted_tokens
                        .as_ref()
                        .unwrap()
                        .tokens,
                    50.0
                );
            }
            assert!(
                total <= 50.0 + 10.0 * elapsed.as_secs_f64() + 0.001,
                "groups must share one quota and retain its debt: total={total}, elapsed={elapsed:?}"
            );
            let unscoped = &response.responses[2].granted_r_u_tokens[0];
            assert_eq!(unscoped.granted_tokens.as_ref().unwrap().tokens, 100000.0);
            assert_eq!(unscoped.trickle_time_ms, 0);
        }
        assert!(
            total >= 51.5,
            "the shared quota must refill between batches: {total}"
        );
        let persisted = store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        assert_eq!(persisted.entries[0].mod_revision, written.revision);
        assert_eq!(persisted.entries[0].value, b"10");
        drop(send);
        assert!(stream.message().await.unwrap().is_none());
        server.stop().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_priority() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = Vec::new();
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust(binary)
        } else {
            Server::start(binary)
        }
        .unwrap();
        server.ready().unwrap();
        let mut client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        let configured = |name: &str, priority| {
            let mut group = group(name, u32::MAX);
            group.priority = priority;
            group.ru_stats = None;
            let bucket = group.r_u_settings.as_mut().unwrap().r_u.as_mut().unwrap();
            bucket.tokens = 0.0;
            bucket.settings.as_mut().unwrap().fill_rate = 100;
            bucket.settings.as_mut().unwrap().burst_limit = 100;
            group
        };
        for group in [configured("high", 16), configured("low", 1)] {
            client
                .add_resource_group(p::PutResourceGroupRequest { group: Some(group) })
                .await
                .unwrap();
        }
        let store = EtcdMetaStore::connect(&[server.metadata_url().into()], Duration::from_secs(3))
            .await
            .unwrap();
        let key = astersql_pd_keypath::keyspace_service_limit_path(u32::MAX);
        store
            .transact(
                vec![],
                vec![astersql_pd_storage_domain::Mutation::put(
                    key.as_bytes(),
                    b"150",
                )],
            )
            .await
            .unwrap();
        if is_rust {
            server.restart_rust(binary).unwrap();
        } else {
            server.restart_with_args(binary, &[]).unwrap();
        }
        server.ready().unwrap();
        client = ResourceManagerClient::connect(server.url.clone())
            .await
            .unwrap();
        let mut request = token_request("high", 1000.0, 99);
        request
            .requests
            .push(token_request("low", 1000.0, 99).requests.remove(0));
        let (send, recv) = tokio::sync::mpsc::channel(1);
        send.send(request.clone()).await.unwrap();
        let mut stream = client
            .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(recv))
            .await
            .unwrap()
            .into_inner();
        stream.message().await.unwrap().unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        exchange(&send, &mut stream, request.clone()).await;
        for (stage, expected) in [
            ("high priority wins basic demand", [100, 50]),
            ("priority changes reverse allocation", [50, 100]),
        ] {
            if stage.starts_with("priority changes") {
                for group in [configured("high", 1), configured("low", 16)] {
                    client
                        .modify_resource_group(p::PutResourceGroupRequest { group: Some(group) })
                        .await
                        .unwrap();
                }
            }
            let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
            loop {
                let response = exchange(&send, &mut stream, request.clone()).await;
                let bursts: Vec<_> = response
                    .responses
                    .iter()
                    .map(|r| {
                        r.granted_r_u_tokens[0]
                            .granted_tokens
                            .as_ref()
                            .unwrap()
                            .settings
                            .as_ref()
                            .unwrap()
                            .burst_limit
                    })
                    .collect();
                records.push(json!({"rust":is_rust,"scenario":stage,"request":format!("{request:?}"),"response":format!("{response:?}"),"burst_limits":bursts}));
                let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap());
                let path = path.with_file_name(format!(
                    "{}-priority.json",
                    path.file_name().unwrap().to_string_lossy()
                ));
                std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
                if bursts == expected {
                    break;
                }
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "priority coordination did not take effect: expected {expected:?}, got {bursts:?}"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        for name in ["high", "low"] {
            let group = client
                .get_resource_group(p::GetResourceGroupRequest {
                    resource_group_name: name.into(),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner()
                .group
                .unwrap();
            let settings = group.r_u_settings.unwrap().r_u.unwrap().settings.unwrap();
            assert_eq!(settings.fill_rate, 100);
            assert_eq!(
                settings.burst_limit, 100,
                "runtime shares must not replace configured settings"
            );
        }
        drop(send);
        assert!(stream.message().await.unwrap().is_none());
        server.stop().unwrap();
    }
}

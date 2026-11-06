// Copyright 2026 AsterSQL.
use super::accept_extra_rm_service_test::Sidecar;
use astersql_pd_proto::{deadlock as d, keyspacepb as k, tracepb as t};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tonic::{Response, Status, Streaming, transport::Channel};
fn rejected<T>(response: Result<Response<T>, Status>) -> Value {
    let status = match response {
        Err(status) => status,
        Ok(_) => panic!("unregistered service returned success"),
    };
    json!({"code":format!("{:?}",status.code()),"message":status.message()})
}
async fn rejected_stream<T: std::fmt::Debug>(
    response: Result<Response<Streaming<T>>, Status>,
) -> Value {
    match response {
        Err(status) => rejected::<()>(Err(status)),
        Ok(response) => rejected(response.into_inner().message().await.map(Response::new)),
    }
}
fn http_missing(url: &str) -> Value {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "2",
            "-w",
            "\n%{http_code}",
            &format!("{url}/not-registered"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    json!({"status":status.parse::<u16>().unwrap(),"body":body})
}
async fn observe(channel: Channel) -> Value {
    let mut deadlock = d::deadlock_client::DeadlockClient::new(channel.clone());
    let mut trace = t::trace_record_pub_sub_client::TraceRecordPubSubClient::new(channel.clone());
    let mut namespace = k::namespace_client::NamespaceClient::new(channel);
    let mut rows = serde_json::Map::new();
    rows.insert(
        "deadlock.Deadlock/GetWaitForEntries".into(),
        rejected(
            deadlock
                .get_wait_for_entries(d::WaitForEntriesRequest::default())
                .await,
        ),
    );
    rows.insert(
        "deadlock.Deadlock/Detect".into(),
        rejected_stream(
            deadlock
                .detect(tokio_stream::iter([d::DeadlockRequest::default()]))
                .await,
        )
        .await,
    );
    rows.insert(
        "tracepb.TraceRecordPubSub/Subscribe".into(),
        rejected_stream(trace.subscribe(t::TraceRecordRequest::default()).await).await,
    );
    rows.insert(
        "keyspacepb.Namespace/CreateNamespace".into(),
        rejected(
            namespace
                .create_namespace(k::CreateNamespaceRequest::default())
                .await,
        ),
    );
    rows.insert(
        "keyspacepb.Namespace/LoadNamespace".into(),
        rejected(
            namespace
                .load_namespace(k::LoadNamespaceRequest::default())
                .await,
        ),
    );
    rows.insert(
        "keyspacepb.Namespace/GetAllNamespaces".into(),
        rejected(
            namespace
                .get_all_namespaces(k::GetAllNamespacesRequest::default())
                .await,
        ),
    );
    rows.insert(
        "keyspacepb.Namespace/UpdateNamespaceState".into(),
        rejected(
            namespace
                .update_namespace_state(k::UpdateNamespaceStateRequest::default())
                .await,
        ),
    );
    Value::Object(rows)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_descriptor_only_services_remain_unregistered_on_all_listener_roles() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut classic = if is_rust {
            crate::harness::Server::start_rust(binary)
        } else {
            crate::harness::Server::start(binary)
        }
        .unwrap();
        classic.ready().unwrap();
        records.push(json!({"rust":is_rust,"role":"classic","http_unknown":http_missing(&classic.url),"responses":observe(Channel::from_shared(classic.url.clone()).unwrap().connect().await.unwrap()).await}));
        classic.stop().unwrap();
        let mut owner = crate::harness::Server::start(&go).unwrap();
        owner.ready().unwrap();
        for role in ["resource-manager", "router", "scheduling", "tso"] {
            let mut sidecar = Sidecar::start_service(binary, &owner.url, role, "http");
            let deadline = Instant::now() + Duration::from_secs(12);
            let channel = loop {
                assert!(
                    sidecar.child.try_wait().unwrap().is_none(),
                    "{}",
                    sidecar.logs()
                );
                if let Ok(channel) = Channel::from_shared(sidecar.url.clone())
                    .unwrap()
                    .connect()
                    .await
                {
                    break channel;
                }
                assert!(Instant::now() < deadline, "MCS connect: {}", sidecar.logs());
                tokio::time::sleep(Duration::from_millis(30)).await;
            };
            let responses = tokio::time::timeout(Duration::from_secs(5), observe(channel))
                .await
                .unwrap();
            records.push(json!({"rust":is_rust,"role":role,"http_unknown":http_missing(&sidecar.url),"responses":responses}));
            sidecar.stop();
        }
        owner.stop().unwrap();
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("unregistered.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    for row in &records {
        assert_eq!(row["http_unknown"]["status"], 404, "HTTP fallback: {row}");
        assert_eq!(row["responses"].as_object().unwrap().len(), 7);
        for (_, response) in row["responses"].as_object().unwrap() {
            assert_eq!(
                response["code"], "Unimplemented",
                "unregistered boundary: {row}"
            );
        }
    }
    for (go, rust) in records[..5].iter().zip(&records[5..]) {
        assert_eq!(go["role"], rust["role"]);
        assert_eq!(
            go["responses"], rust["responses"],
            "unregistered responses: {}",
            go["role"]
        );
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_mcs_signal_exit_codes_and_registry_cleanup_match_go() {
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut owner = crate::harness::Server::start(&go).unwrap();
    owner.ready().unwrap();
    let mut pd = astersql_pd_proto::pdpb::pd_client::PdClient::connect(owner.url.clone())
        .await
        .unwrap();
    let cluster = pd
        .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let store = EtcdMetaStore::connect(&[owner.url.clone()], Duration::from_secs(2))
        .await
        .unwrap();
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        for role in ["resource-manager", "router", "scheduling", "tso"] {
            for signal in ["TERM", "INT", "HUP", "QUIT"] {
                let mut sidecar = Sidecar::start_service(binary, &owner.url, role, "http");
                let key =
                    ClusterPaths::new(cluster).registry_path(&role.replace('-', "_"), &sidecar.url);
                let deadline = Instant::now() + Duration::from_secs(12);
                let initial = loop {
                    assert!(
                        sidecar.child.try_wait().unwrap().is_none(),
                        "{}",
                        sidecar.logs()
                    );
                    let rows = store
                        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                        .await
                        .unwrap();
                    if !rows.entries.is_empty() {
                        break rows;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "signal registry startup: {}",
                        sidecar.logs()
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                };
                // Registry publication precedes completion of Go's store watch
                // initialization. Probe the real listener before testing a
                // running service's shutdown rather than startup cancellation.
                loop {
                    assert!(
                        sidecar.child.try_wait().unwrap().is_none(),
                        "{}",
                        sidecar.logs()
                    );
                    if let Ok(mut health) =
                        astersql_pd_proto::health::health_client::HealthClient::connect(
                            sidecar.url.clone(),
                        )
                        .await
                    {
                        if health
                            .check(astersql_pd_proto::health::HealthCheckRequest::default())
                            .await
                            .is_ok()
                        {
                            break;
                        }
                    }
                    assert!(
                        Instant::now() < deadline,
                        "signal listener startup: {}",
                        sidecar.logs()
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                assert!(
                    std::process::Command::new("kill")
                        .args([&format!("-{signal}"), &sidecar.child.id().to_string()])
                        .status()
                        .unwrap()
                        .success()
                );
                let deadline = Instant::now() + Duration::from_secs(8);
                let status = loop {
                    if let Some(status) = sidecar.child.try_wait().unwrap() {
                        break status;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "signal cleanup timeout: {}",
                        sidecar.logs()
                    );
                    tokio::time::sleep(Duration::from_millis(10)).await;
                };
                let remaining = store
                    .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                    .await
                    .unwrap();
                let address = sidecar.url.strip_prefix("http://").unwrap();
                let listener_closed = tokio::net::TcpStream::connect(address).await.is_err();
                records.push(json!({"rust":is_rust,"role":role,"signal":signal,"exit_code":status.code(),"exit_status":status.to_string(),"initial_registry_lease":initial.entries[0].lease,"remaining_registry":remaining.entries.len(),"listener_closed":listener_closed,"logs":sidecar.logs()}));
            }
        }
    }
    drop(store);
    drop(pd);
    owner.stop().unwrap();
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("signals.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    for row in records {
        assert_eq!(
            row["exit_code"],
            if row["signal"] == "TERM" { 0 } else { 1 },
            "MCS signal status: {row}"
        );
        assert_eq!(
            row["remaining_registry"], 0,
            "MCS signal registry cleanup: {row}"
        );
        assert_eq!(row["listener_closed"], true);
    }
}

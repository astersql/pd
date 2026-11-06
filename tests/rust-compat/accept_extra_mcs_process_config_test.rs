// Copyright 2026 AsterSQL.
use super::{accept_extra_rm_service_test::Sidecar, accept_extra_transport_config_test::Work};
use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};
fn granted_ttl(endpoint: &str, id: i64) -> Value {
    let output = Command::new(std::env::var_os("PD_ETCDCTL_BIN").unwrap())
        .args([
            "--endpoints",
            endpoint,
            "--dial-timeout=2s",
            "--command-timeout=2s",
            "lease",
            "timetolive",
            &format!("{id:x}"),
            "-w",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_mcs_file_lease_and_role_default_name_are_observable_in_etcd() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let work =
        Work(std::env::temp_dir().join(format!("pd-mcs-process-config-{}", std::process::id())));
    std::fs::create_dir(&work.0).unwrap();
    let cfg = work.0.join("process.toml");
    let mut owner = crate::harness::Server::start_with_environment(
        &go,
        &[],
        &[("PD_SERVICE_MODE".into(), "api".into())],
    )
    .unwrap();
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
    use astersql_pd_proto::{metapb, pdpb};
    let bootstrapped = pd
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: cluster,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
                address: "process-config-store".into(),
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
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(bootstrapped.header.unwrap().error.is_none());
    let store = EtcdMetaStore::connect(&[owner.url.clone()], Duration::from_secs(2))
        .await
        .unwrap();
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        for configured in [0, 9] {
            std::fs::write(&cfg, format!("lease={configured}\n")).unwrap();
            let arg = format!("--config={}", cfg.display());
            for service in ["resource-manager", "router", "scheduling", "tso"] {
                let mut sidecar = Sidecar::start_service_with_args(
                    binary,
                    &owner.url,
                    service,
                    "http",
                    &[&arg, "--name="],
                );
                let deadline = Instant::now() + Duration::from_secs(12);
                let health = loop {
                    if let Some(status) = sidecar.child.try_wait().unwrap() {
                        break json!({"ready":false,"exit":status.to_string(),"logs":sidecar.logs()});
                    }
                    if let Ok(mut client) =
                        astersql_pd_proto::health::health_client::HealthClient::connect(
                            sidecar.url.clone(),
                        )
                        .await
                    {
                        if client
                            .check(astersql_pd_proto::health::HealthCheckRequest::default())
                            .await
                            .is_ok()
                        {
                            break json!({"ready":true});
                        }
                    }
                    assert!(
                        Instant::now() < deadline,
                        "sidecar ready: {}",
                        sidecar.logs()
                    );
                    tokio::time::sleep(Duration::from_millis(30)).await;
                };
                let name = service.replace('-', "_");
                let paths = ClusterPaths::new(cluster);
                let registry = paths.registry_path(&name, &sidecar.url);
                let registry_deadline = Instant::now() + Duration::from_secs(3);
                let entries = loop {
                    let entries = store
                        .read(Read::range(
                            registry.as_bytes(),
                            b"",
                            Consistency::Linearizable,
                        ))
                        .await
                        .unwrap()
                        .entries;
                    if !entries.is_empty()
                        || health["ready"] != true
                        || Instant::now() >= registry_deadline
                    {
                        break entries;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                };
                let mut row = json!({"rust":is_rust,"configured_lease":configured,"service":service,"health":health});
                if let Some(entry) = entries.first() {
                    row["entry"] = serde_json::from_slice(&entry.value).unwrap();
                    row["registry_ttl"] = granted_ttl(&owner.url, entry.lease.unwrap());
                }
                if service != "router" {
                    let key = paths.election_path(Some(&MsParam {
                        service_name: name,
                        group_id: 0,
                    }));
                    let deadline = Instant::now() + Duration::from_secs(8);
                    loop {
                        let entries = store
                            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                            .await
                            .unwrap()
                            .entries;
                        if let Some(entry) = entries.first() {
                            row["primary_ttl"] = granted_ttl(&owner.url, entry.lease.unwrap());
                            break;
                        }
                        if Instant::now() >= deadline {
                            row["primary_missing"] = json!(true);
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(30)).await;
                    }
                }
                if health["ready"] == true {
                    sidecar.stop();
                }
                records.push(row);
            }
        }
    }
    drop(store);
    drop(pd);
    owner.stop().unwrap();
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("process-config.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    for row in &records {
        assert_eq!(row["health"]["ready"], true, "process configuration: {row}");
        assert_eq!(row["registry_ttl"]["granted-ttl"], 5, "registry TTL: {row}");
        let expected = if row["configured_lease"] == 0 { 5 } else { 9 };
        if row["service"] != "router" {
            assert_eq!(
                row["primary_ttl"]["granted-ttl"], expected,
                "primary TTL: {row}"
            );
        }
    }
    for (go, rust) in records[..8].iter().zip(&records[8..]) {
        assert_eq!(go["service"], rust["service"]);
        assert_eq!(
            go["entry"]["name"], rust["entry"]["name"],
            "role default name: {go} / {rust}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_mcs_controller_types_only_apply_to_resource_manager() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let work =
        Work(std::env::temp_dir().join(format!("pd-mcs-controller-role-{}", std::process::id())));
    std::fs::create_dir(&work.0).unwrap();
    let cfg = work.0.join("role.toml");
    std::fs::write(&cfg, "[controller]\nenable-controller-trace-log=42\n").unwrap();
    let arg = format!("--config={}", cfg.display());
    let mut owner = crate::harness::Server::start(&go).unwrap();
    owner.ready().unwrap();
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        for service in ["resource-manager", "router", "scheduling", "tso"] {
            let mut sidecar =
                Sidecar::start_service_with_args(binary, &owner.url, service, "http", &[&arg]);
            let deadline = Instant::now() + Duration::from_secs(12);
            let accepted = loop {
                if sidecar.child.try_wait().unwrap().is_some() {
                    break false;
                }
                if let Ok(mut client) =
                    astersql_pd_proto::health::health_client::HealthClient::connect(
                        sidecar.url.clone(),
                    )
                    .await
                {
                    if client
                        .check(astersql_pd_proto::health::HealthCheckRequest::default())
                        .await
                        .is_ok()
                    {
                        break true;
                    }
                }
                assert!(
                    Instant::now() < deadline,
                    "role config startup: {}",
                    sidecar.logs()
                );
                tokio::time::sleep(Duration::from_millis(30)).await;
            };
            if accepted {
                sidecar.stop();
            }
            records.push(
                json!({"rust":is_rust,"service":service,"accepted":accepted,"logs":sidecar.logs()}),
            );
        }
    }
    owner.stop().unwrap();
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("controller-role.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    for row in records {
        assert_eq!(
            row["accepted"],
            row["service"] != "resource-manager",
            "role config: {row}"
        );
    }
}

// Copyright 2026 AsterSQL.
use super::{
    accept_extra_mcs_tls_test::ready, accept_extra_rm_service_test::Sidecar,
    accept_extra_transport_config_test::certificates,
};
use astersql_pd_keypath::ClusterPaths;
use serde_json::json;
use std::path::PathBuf;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_rm_https_backend_uses_native_etcd_and_owned_sdk_transport() {
    let certs = certificates("mcs-backend-tls");
    let cfg = certs.0.join("backend.toml");
    std::fs::write(
        &cfg,
        format!(
            "[security]\ncacert-path={}\ncert-path={}\nkey-path={}\n",
            json!(certs.0.join("ca.pem")),
            json!(certs.0.join("server.pem")),
            json!(certs.0.join("server.key"))
        ),
    )
    .unwrap();
    let arg = format!("--config={}", cfg.display());
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        for profile in ["https", "bare-tls", "http-with-credentials"] {
            let backend = if profile == "http-with-credentials" {
                crate::harness::fixture::Fixture::start()
            } else {
                crate::harness::fixture::Fixture::start_with_tls(&certs.0)
            };
            let endpoint = if profile == "bare-tls" {
                backend.endpoint.strip_prefix("https://").unwrap()
            } else {
                &backend.endpoint
            };
            let cluster = 160_u64;
            let seed = backend.native_ctl(&["put", "/pd/cluster_id"], Some(&cluster.to_be_bytes()));
            assert!(
                seed.status.success(),
                "native cluster identity seed: {}",
                String::from_utf8_lossy(&seed.stderr)
            );
            let before = backend.native_ctl(&["get", "/pd/cluster_id", "-w", "json"], None);
            assert!(before.status.success());
            let mut sidecar = Sidecar::start_service_with_args(
                binary,
                endpoint,
                "resource-manager",
                "https",
                &[&arg],
            );
            let response = ready(&mut sidecar, &certs.0).await;
            let key = ClusterPaths::new(cluster).registry_path(
                "resource_manager",
                &sidecar.url.replacen("http://", "https://", 1),
            );
            let registry = backend.native_ctl(&["get", &key, "-w", "json"], None);
            assert!(registry.status.success());
            let registry: serde_json::Value = serde_json::from_slice(&registry.stdout).unwrap();
            records.push(json!({"rust":is_rust,"profile":profile,"configured_endpoint":endpoint,"cluster_id":cluster,"backend":backend.endpoint,"ready":response,"seed_stdout":String::from_utf8_lossy(&seed.stdout),"identity_snapshot":serde_json::from_slice::<serde_json::Value>(&before.stdout).unwrap(),"registration":registry}));
            if response["accepted"] == true {
                sidecar.stop();
                let removed = backend.native_ctl(&["get", &key, "-w", "json"], None);
                assert!(removed.status.success());
                let removed: serde_json::Value = serde_json::from_slice(&removed.stdout).unwrap();
                records.last_mut().unwrap()["registration_after_shutdown"] = removed;
            }
        }
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("backend-tls.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    for record in &records {
        if record["profile"] == "bare-tls" {
            assert_eq!(record["ready"]["accepted"], false);
            assert!(record["registration"]["kvs"].is_null());
            continue;
        }
        assert_eq!(
            record["ready"]["accepted"], true,
            "backend TLS registration: {record}"
        );
        let registrations = record["registration"]["kvs"].as_array().unwrap();
        assert_eq!(registrations.len(), 1);
        assert!(registrations[0]["lease"].as_i64().unwrap() > 0);
        assert!(record["registration_after_shutdown"]["kvs"].is_null());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_four_mcs_https_backend_uses_real_pd_owner_and_cleans_registry() {
    use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
    let certs = certificates("mcs-https-owner");
    let cfg = certs.0.join("owner.toml");
    std::fs::write(
        &cfg,
        format!(
            "[security]\ncacert-path={}\ncert-path={}\nkey-path={}\n",
            json!(certs.0.join("ca.pem")),
            json!(certs.0.join("server.pem")),
            json!(certs.0.join("server.key"))
        ),
    )
    .unwrap();
    let config_arg = format!("--config={}", cfg.display());
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("https://{}", reserved.local_addr().unwrap());
    drop(reserved);
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut owner = crate::harness::Server::start_with_args(
        &go,
        &[
            &config_arg,
            &format!("--client-urls={address}"),
            &format!("--advertise-client-urls={address}"),
        ],
    )
    .unwrap();
    owner.url = address.clone();
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let response = Command::new("curl")
            .args(["-fsS", "--max-time", "1", "--cacert"])
            .arg(certs.0.join("ca.pem"))
            .arg("--cert")
            .arg(certs.0.join("allowed.pem"))
            .arg("--key")
            .arg(certs.0.join("allowed.key"))
            .arg(format!("{address}/pd/api/v1/members"))
            .output()
            .unwrap();
        if response.status.success() && serde_json::from_slice::<serde_json::Value>(&response.stdout).unwrap()["leader"]["member_id"].as_u64().is_some_and(|id|id>0) {break;}
        assert!(
            Instant::now() < deadline,
            "real HTTPS PD owner startup: {}",
            owner.logs()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(
            std::fs::read(certs.0.join("ca.pem")).unwrap(),
        ))
        .identity(Identity::from_pem(
            std::fs::read(certs.0.join("allowed.pem")).unwrap(),
            std::fs::read(certs.0.join("allowed.key")).unwrap(),
        ));
    let endpoint = Endpoint::from_shared(address.clone())
        .unwrap()
        .tls_config(tls)
        .unwrap();
    let mut pd =
        astersql_pd_proto::pdpb::pd_client::PdClient::new(endpoint.connect().await.unwrap());
    let members = pd
        .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    let cluster = members.header.unwrap().cluster_id;
    assert!(members.leader.unwrap().member_id > 0);
    let store = EtcdMetaStore::connect_with_transport_factory(
        &[address.clone()],
        Duration::from_secs(2),
        move |_| Ok(endpoint.connect_lazy()),
    )
    .await
    .unwrap();
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        for service in ["resource-manager", "router", "scheduling", "tso"] {
            let mut sidecar = Sidecar::start_service_with_args(
                binary,
                &address,
                service,
                "https",
                &[&config_arg],
            );
            let readiness = ready(&mut sidecar, &certs.0).await;
            let advertised = sidecar.url.replacen("http://", "https://", 1);
            let key =
                ClusterPaths::new(cluster).registry_path(&service.replace('-', "_"), &advertised);
            let registered = store
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap();
            let mut record = json!({"rust":is_rust,"service":service,"owner":address,"cluster_id":cluster,"ready":readiness,"registered":registered.entries.len(),"leases":registered.entries.iter().map(|e|e.lease).collect::<Vec<_>>()});
            if readiness["accepted"] == true {
                sidecar.stop();
                record["remaining_after_shutdown"] = json!(
                    store
                        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                        .await
                        .unwrap()
                        .entries
                        .len()
                );
            }
            records.push(record);
        }
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("https-owner.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    drop(store);
    drop(pd);
    owner.stop().unwrap();
    for row in records {
        assert_eq!(row["ready"]["accepted"], true, "HTTPS backend: {row}");
        assert_eq!(row["registered"], 1);
        assert!(row["leases"][0].as_i64().unwrap() > 0);
        assert_eq!(row["remaining_after_shutdown"], 0);
    }
}

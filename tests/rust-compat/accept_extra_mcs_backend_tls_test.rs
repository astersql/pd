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

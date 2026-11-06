// Copyright 2026 AsterSQL.
#[allow(dead_code)]
#[path = "../../pkg/storage/kv/contract_test.rs"]
mod contract;
#[allow(dead_code)]
#[path = "../../pkg/storage/kv/etcd_fixture_test.rs"]
mod fixture;
use astersql_pd_storage::{Consistency, MetaStore, Read, StoreError};
use std::{path::PathBuf, process::Command};
use tokio_util::sync::CancellationToken;
struct Certificates(PathBuf);
impl Drop for Certificates {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_tls_sdk_preserves_storage_contract_and_endpoint_health_isolation() {
    let certs = Certificates(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join(format!("backend-tls-contract-{}", std::process::id())),
    );
    assert!(
        Command::new("python3")
            .args(["../../scripts/service-auth-oracle.py", "--certificates"])
            .arg(&certs.0)
            .status()
            .unwrap()
            .success()
    );
    let backend = fixture::Fixture::start_with_tls(&certs.0);
    let mut config = crate::mcs_config::Config {
        backend_endpoints: backend.endpoint.clone(),
        ..Default::default()
    };
    config.security.tls.ca_path = certs.0.join("ca.pem").display().to_string();
    config.security.tls.cert_path = certs.0.join("server.pem").display().to_string();
    config.security.tls.key_path = certs.0.join("server.key").display().to_string();
    let cancel = CancellationToken::new();
    let _lifetime = cancel.clone().drop_guard();
    let store = super::backend_transport::connect(&config, &cancel)
        .await
        .unwrap();
    contract::cas_atomic_revision(&store).await;
    contract::watch_replay(&store).await;
    contract::compaction_recovery(&store).await;
    contract::lease_renew_revoke(&store).await;
    assert!(store.backend_health().await[0].healthy);
    let reserved = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead = format!("https://{}", reserved.local_addr().unwrap());
    drop(reserved);
    config.backend_endpoints = format!("{dead},{}", backend.endpoint);
    let mixed = super::backend_transport::connect(&config, &cancel)
        .await
        .unwrap();
    assert!(
        !mixed
            .read(Read::prefix(b"/compact/", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    let health = mixed.backend_health().await;
    assert_eq!(health.len(), 2);
    assert_eq!(health[0].endpoint, dead);
    assert!(!health[0].healthy);
    assert_eq!(health[1].endpoint, backend.endpoint);
    assert!(health[1].healthy);
    cancel.cancel();
    assert_eq!(
        store
            .read(Read::prefix(b"/", Consistency::Linearizable))
            .await
            .unwrap_err(),
        StoreError::Unavailable
    );
    assert!(
        mixed
            .backend_health()
            .await
            .iter()
            .all(|endpoint| !endpoint.healthy)
    );
}

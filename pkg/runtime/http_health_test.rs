// Copyright 2026 AsterSQL.
use astersql_pd_server::PdServices;
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use astersql_pd_storage_endpoint::{Cluster, StorageEndpoint};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

fn get(url: &str, path: &str) -> (u16, String) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-w",
            "\n%{http_code}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    (code.parse().unwrap(), body.to_owned())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_043_listener_loading_error_and_cancel() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let services = Arc::new(crate::member_role_worker::RoleServices::new(
        crate::NodeConfig {
            cluster_id: 7431,
            member_id: 2,
            name: "loading".into(),
            client_url: url.clone(),
            peer_url: "http://peer2".into(),
        },
        store.clone(),
    ));
    // Merely binding a socket must not mark the cluster/listener service ready.
    assert!(services.probe_status().await.is_err());
    let cancel = CancellationToken::new();
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let router = astersql_pd_server::router(services.clone(), cancel.clone());
    let worker = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    assert_eq!(get(&url, "/pd/api/v2/ready").0, 500);
    assert_eq!(get(&url, "/pd/api/v2/readyz?exclude=etcd-data-corruption&exclude=etcd-linearizable-read&exclude=etcd-non-learner&exclude=etcd-serializable-read&exclude=leader-promotion").0,503);
    services.set_listening(true);
    assert_eq!(get(&url, "/pd/api/v2/ready").0, 200);
    let (code, body) = get(&url, "/pd/api/v2/readyz?verbose");
    assert_eq!(code, 503);
    assert!(body.ends_with("[+]leader-promotion ok\n"));
    assert_eq!(
        body.matches("local consensus diagnostics are not configured")
            .count(),
        4
    );
    assert_eq!(get(&url, "/pd/api/v2/livez").0, 503);
    assert_eq!(
        get(
            &url,
            "/pd/api/v2/readyz/etcd-non-learner?exclude=etcd-non-learner"
        )
        .0,
        503
    );
    assert_eq!(get(&url, "/pd/api/v2/livez/leader-promotion").0, 404);
    assert_eq!(get(&url, "/pd/api/v2/readyz/unknown").0, 404);
    assert_eq!(
        get(
            &url,
            "/pd/api/v2/livez?exclude=etcd-serializable-read&verbose"
        ),
        (200, "ok\n".into())
    );
    let paths = astersql_pd_keypath::ClusterPaths::new(7431);
    StorageEndpoint::new(store.clone())
        .save_meta(
            &paths,
            &Cluster {
                id: 7431,
                max_peer_count: 3,
            },
        )
        .await
        .unwrap();
    let before = store
        .read(Read::prefix(b"/pd/7431/", Consistency::Linearizable))
        .await
        .unwrap();
    let (code, body) = get(&url, "/pd/api/v2/ready?verbose=false");
    assert_eq!(code, 500);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"region_loaded":false,"is_bootstrapped":true})
    );
    assert_eq!(
        get(&url, "/pd/api/v2/readyz/leader-promotion"),
        (
            503,
            "[-]leader-promotion failed: regions not loaded\n".into()
        )
    );
    // No leader is present: these responses must be local, not redirected.
    let after = store
        .read(Read::prefix(b"/pd/7431/", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(before.entries, after.entries);
    store
        .transact(
            vec![],
            vec![Mutation::put(paths.cluster_path().as_bytes(), b"\xff")],
        )
        .await
        .unwrap();
    assert_eq!(get(&url, "/pd/api/v2/ready?verbose").0, 500);
    cancel.cancel();
    assert_eq!(get(&url, "/pd/api/v2/ready").0, 500);
    assert_eq!(get(&url, "/pd/api/v2/readyz/leader-promotion").0, 503);
    assert_eq!(get(&url, "/pd/health").0, 503);
    assert_eq!(get(&url, "/pd/ping").0, 503);
    services.set_listening(false);
    assert!(services.probe_status().await.is_err());
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap();
}

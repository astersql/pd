// Copyright 2026 AsterSQL.
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use astersql_pd_storage::{MetaStore, Mutation};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
fn call(url: &str, method: &str, path: &str, body: &str) -> (u16, Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v2/maintenance{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), serde_json::from_str(body).unwrap())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_102_maintenance_persistence_and_conflicts() {
    maintenance_contract(
        Arc::new(astersql_pd_storage_kv::MemoryStore::default()),
        None,
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_102_maintenance_etcd_wal_restart() {
    let mut fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    maintenance_contract(store, Some(&mut fixture)).await;
}

async fn maintenance_contract(
    mut store: Arc<dyn MetaStore>,
    mut fixture: Option<&mut crate::etcd_fixture::Fixture>,
) {
    let initial = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = initial.local_addr().unwrap();
    drop(initial);
    for restart in [false, true] {
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let registry = Arc::new(prometheus::Registry::new());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 778,
                        member_id: 1,
                        name: "maintenance".into(),
                        client_url: url.clone(),
                        peer_url: "http://peer".into(),
                    },
                    listener,
                    registry.clone(),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        if !restart {
            assert_eq!(call(&url, "GET", "", "").0, 404);
            assert_eq!(call(&url, "PUT", "/upgrade/one", "维护 <>&").0, 200);
            let metrics = registry.gather();
            let family = metrics
                .iter()
                .find(|m| m.name() == "pd_maintenance_task_info")
                .unwrap();
            assert_eq!(family.get_metric()[0].get_gauge().get_value(), 1.0);
            let task = call(&url, "GET", "/upgrade", "");
            assert_eq!(task.0, 200);
            assert_eq!(task.1["description"], "维护 <>&");
            assert!(task.1["start_timestamp"].as_i64().unwrap() > 0);
            let conflict = call(&url, "PUT", "/other/two", "");
            assert_eq!(conflict.0, 409);
            assert_eq!(conflict.1["existing_task"], task.1);
            assert_eq!(call(&url, "DELETE", "/upgrade/wrong", "").0, 409);
            assert_eq!(call(&url, "DELETE", "/missing/one", "").0, 404);
        } else {
            let tasks = call(&url, "GET", "", "");
            assert_eq!(tasks.0, 200);
            assert_eq!(tasks.1.as_array().unwrap().len(), 1);
            assert_eq!(tasks.1[0]["id"], "one");
            assert_eq!(call(&url, "DELETE", "/upgrade/one", "").0, 200);
            assert_eq!(call(&url, "GET", "", "").0, 404);
            assert_eq!(call(&url, "PUT", "/other/two", "").0, 200);
            assert_eq!(call(&url, "DELETE", "/other/two", "").0, 200);
            assert_eq!(
                call(&url, "GET", "/other", "").1,
                json!("No maintenance task is running for this type")
            );
        }
        if restart {
            let key = ClusterPaths::new(778)
                .maintenance_task_path("broken")
                .into_bytes();
            store
                .transact(vec![], vec![Mutation::put(&key, b"broken json")])
                .await
                .unwrap();
            assert_eq!(call(&url, "GET", "/broken", "").0, 500);
            assert_eq!(call(&url, "GET", "", "").0, 404);
            store
                .transact(vec![], vec![Mutation::Delete { key }])
                .await
                .unwrap();
            let mut requests = Vec::new();
            for id in 0..8 {
                let url = url.clone();
                requests.push(tokio::task::spawn_blocking(move || {
                    (id, call(&url, "PUT", &format!("/race/{id}"), ""))
                }));
            }
            let mut winners = Vec::new();
            for request in requests {
                let (id, result) = request.await.unwrap();
                if result.0 == 200 {
                    winners.push(id);
                } else {
                    assert_eq!(result.0, 409);
                }
            }
            assert_eq!(winners.len(), 1);
            assert_eq!(
                call(&url, "DELETE", &format!("/race/{}", winners[0]), "").0,
                200
            );
        }
        node.shutdown().await.unwrap();
        assert!(
            registry.gather().is_empty(),
            "host collectors must be released"
        );
        println!("maintenance host registry empty after shutdown (restart={restart})");
        if !restart {
            if let Some(fixture) = fixture.as_mut() {
                fixture.command("stop", "stopped");
                fixture.command("start", "restarted");
                store = Arc::new(fixture.store().await);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_102_maintenance_frozen_go_contract() {
    let oracle: Value = serde_json::from_str(include_str!(
        "../../tests/rust-compat/fixtures/task-102-maintenance-go.json"
    ))
    .unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            Arc::new(fixture.store().await),
            NodeConfig {
                cluster_id: 779,
                member_id: 1,
                name: "maintenance-oracle".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut timestamp = None;
    for row in oracle["observations"].as_array().unwrap() {
        let out = std::process::Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "5",
                "-X",
                row["method"].as_str().unwrap(),
                "--data-binary",
                oracle["request_body"].as_str().unwrap(),
                "-w",
                "\n%{http_code}\n%{content_type}",
                &format!("{url}/pd/api/v2/{}", row["path"].as_str().unwrap()),
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        let text = String::from_utf8(out.stdout).unwrap();
        let mut parts = text.rsplitn(3, '\n');
        let content_type = parts.next().unwrap();
        let status: u64 = parts.next().unwrap().parse().unwrap();
        let mut actual: Value = serde_json::from_str(parts.next().unwrap()).unwrap();
        let expected: Value = serde_json::from_str(row["go"]["body"].as_str().unwrap()).unwrap();
        assert_eq!(status, row["go"]["status"].as_u64().unwrap());
        assert_eq!(content_type, row["go"]["content_type"].as_str().unwrap());
        let (task, expected_task) = if actual.is_array() {
            (actual.get_mut(0).unwrap(), &expected[0])
        } else {
            (&mut actual, &expected)
        };
        if let Some(value) = task.get_mut("start_timestamp") {
            let current = value.as_i64().unwrap();
            assert!(current > 0);
            assert_eq!(
                *timestamp.get_or_insert(current),
                current,
                "both reads must observe the same persisted timestamp"
            );
            *value = expected_task["start_timestamp"].clone();
        }
        assert_eq!(actual, expected);
    }
    node.shutdown().await.unwrap();
}

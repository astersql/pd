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
    let store = Arc::new(astersql_pd_storage_kv::MemoryStore::default());
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
    }
}

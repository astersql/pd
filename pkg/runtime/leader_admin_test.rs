// Copyright 2026 AsterSQL.
use crate::etcd_diagnostics_test::{address, launch, wait_status};
fn request(url: &str, method: &str, path: &str, _body: &str) -> (u16, serde_json::Value) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "15",
            "-X",
            method,
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    assert!(matches!(
        content,
        "application/json; charset=UTF-8" | "text/plain; charset=utf-8"
    ));
    (
        status.parse().unwrap(),
        serde_json::from_str(body).unwrap_or(serde_json::json!(body)),
    )
}
use crate::{NodeConfig, PdNodeBuilder, member_admin::EtcdMemberAdmin};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

async fn pd_leader(url: &str, expected: Option<u64>) -> u64 {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let (status, body) = request(url, "GET", "/pd/api/v1/leader", "");
            if status == 200 {
                if let Some(id) = body["member_id"].as_u64() {
                    if expected.is_none_or(|expected| id == expected) {
                        return id;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap()
}
async fn ready(node: &crate::PdNode) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while node.try_cluster().is_none() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 6)]
#[ignore = "requires verified standalone etcd through PD_ETCD_BIN"]
async fn task_045_real_transfer_and_fencing() {
    let binary = std::env::var("PD_ETCD_BIN").unwrap();
    let urls: Vec<_> = (0..3).map(|_| address()).collect();
    let peers: Vec<_> = (0..3).map(|_| address()).collect();
    let initial = peers
        .iter()
        .enumerate()
        .map(|(i, url)| format!("member-{}={url}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let mut processes: Vec<_> = (0..3)
        .map(|i| {
            Some(launch(
                &binary,
                &format!("member-{}", i + 1),
                &urls[i],
                &peers[i],
                &initial,
                false,
            ))
        })
        .collect();
    let mut clients = Vec::new();
    let mut ids = Vec::new();
    for url in &urls {
        let mut client = wait_status(url).await;
        ids.push(client.status().await.unwrap().header().unwrap().member_id());
        clients.push(client);
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let leader = clients[0].status().await.unwrap().leader();
            let mut agreed = leader != 0;
            for client in &mut clients {
                agreed &= client.status().await.unwrap().leader() == leader;
            }
            if agreed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    let store: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage::EtcdMetaStore::connect(&urls[..1], Duration::from_secs(3))
            .await
            .unwrap(),
    );
    let mut nodes = Vec::new();
    let mut pd_urls = Vec::new();
    for i in 0..3 {
        let provider = Arc::new(
            EtcdMemberAdmin::connect(urls[i].clone(), ids[i], Duration::from_secs(3))
                .await
                .unwrap(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .with_etcd_member_admin(provider)
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 7450,
                        member_id: i as u64 + 1,
                        name: format!("member-{}", i + 1),
                        client_url: url.clone(),
                        peer_url: peers[i].clone(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap_or_else(|e| panic!("node {} startup: {e:?}", i + 1));
        node.ready().await.unwrap();
        nodes.push(node);
        pd_urls.push(url);
    }
    let old = pd_leader(&pd_urls[0], None).await as usize - 1;
    let target = (old + 1) % 3;
    ready(&nodes[old]).await;
    let old_cluster = nodes[old].cluster();
    let leader_response = request(&pd_urls[old], "GET", "/pd/api/v1/leader", "");
    let self_transfer = request(
        &pd_urls[old],
        "POST",
        &format!("/pd/api/v1/leader/transfer/member-{}", old + 1),
        "",
    );
    assert_eq!(
        self_transfer,
        (200, serde_json::json!("The transfer command is submitted."))
    );
    let read = Read::range(b"/pd/7450/leader", b"", Consistency::Linearizable);
    let before = store.read(read.clone()).await.unwrap();
    let missing = request(
        &pd_urls[old],
        "POST",
        "/pd/api/v1/leader/transfer/absent",
        "",
    );
    assert_eq!(
        missing,
        (
            500,
            serde_json::json!("no valid pd to transfer etcd leader")
        )
    );
    assert_eq!(
        before.entries,
        store.read(read.clone()).await.unwrap().entries
    );
    let transferred = request(
        &pd_urls[old],
        "POST",
        &format!("/pd/api/v1/leader/transfer/member-{}", target + 1),
        "",
    );
    if transferred.0 != 200 {
        for node in &mut nodes {
            eprintln!("transfer failure shutdown: {:?}", node.shutdown().await);
        }
    }
    assert_eq!(
        transferred,
        (200, serde_json::json!("The transfer command is submitted."))
    );
    pd_leader(&pd_urls[target], Some(target as u64 + 1)).await;
    ready(&nodes[target]).await;
    assert_eq!(
        clients[target].status().await.unwrap().leader(),
        ids[target]
    );
    let after = store.read(read.clone()).await.unwrap();
    assert_ne!(before.entries[0].lease, after.entries[0].lease);
    assert!(after.entries[0].mod_revision > before.entries[0].mod_revision);
    assert!(old_cluster.set_external_timestamp(99).await.is_err());
    assert!(nodes[old].try_cluster().is_none());
    let resigned = request(&pd_urls[target], "POST", "/pd/api/v1/leader/resign", "");
    assert_eq!(
        resigned,
        (200, serde_json::json!("The resign command is submitted."))
    );
    let next = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let id = pd_leader(&pd_urls[0], None).await as usize - 1;
            if id != target {
                break id;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(clients[next].status().await.unwrap().leader(), ids[next]);
    // A stopped target remains registered; consensus must reject it and preserve
    // the serving PD term. Stopping only one of three voters preserves quorum.
    ready(&nodes[next]).await;
    let dead = (0..3).find(|i| *i != next && *i != 0).unwrap();
    // Keep the PD listener alive to exercise actual consensus RPC failure.
    drop(processes[dead].take());
    let before_dead = store.read(read.clone()).await.unwrap();
    let unavailable = request(
        &pd_urls[next],
        "POST",
        &format!("/pd/api/v1/leader/transfer/member-{}", dead + 1),
        "",
    );
    assert_eq!(unavailable.0, 500);
    assert!(
        unavailable
            .1
            .as_str()
            .unwrap()
            .contains("ErrEtcdMoveLeader"),
        "{unavailable:?}"
    );
    assert_eq!(before_dead.entries, store.read(read).await.unwrap().entries);
    assert_eq!(clients[next].status().await.unwrap().leader(), ids[next]);
    if let Ok(path) = std::env::var("PD_HTTP_LEADER_ORACLE") {
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            serde_json::json!(self_transfer),
            oracle["responses"]["self"]
        );
        assert_eq!(
            leader_response.0 as u64,
            oracle["responses"]["get"][0].as_u64().unwrap()
        );
        // Identity and listener addresses differ by deployment; preserve all field
        // names and validate their actual local identity instead of erasing fields.
        assert_eq!(
            leader_response
                .1
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            oracle["responses"]["get"][1]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>()
        );
        assert_eq!(leader_response.1["name"], format!("member-{}", old + 1));
        assert_eq!(leader_response.1["client_urls"][0], pd_urls[old]);
        assert_eq!(leader_response.1["peer_urls"][0], peers[old]);
        assert_eq!(serde_json::json!(missing), oracle["responses"]["missing"]);
        assert_eq!(
            serde_json::json!(transferred),
            oracle["responses"]["transfer"]
        );
        assert_eq!(serde_json::json!(resigned), oracle["responses"]["resign"]);
        assert_eq!(serde_json::json!(unavailable), oracle["responses"]["dead"]);
        assert!(oracle["lease_changed"].as_bool().unwrap());
        println!("task045 real Go/Rust HTTP and lease differential passed");
    }
    // Independent deployment: PD can stop while its dedicated etcd stays alive.
    // Do not hand consensus to an identity which cannot serve the PD role.
    let offline = (0..3).find(|i| *i != next && *i != dead).unwrap();
    nodes[offline].shutdown().await.unwrap();
    let leader_key = Read::range(b"/pd/7450/leader", b"", Consistency::Linearizable);
    let before_offline = store.read(leader_key.clone()).await.unwrap();
    let offline_response = request(
        &pd_urls[next],
        "POST",
        &format!("/pd/api/v1/leader/transfer/member-{}", offline + 1),
        "",
    );
    assert_eq!(
        offline_response.0, 500,
        "offline PD must not receive consensus leadership"
    );
    assert!(
        offline_response
            .1
            .as_str()
            .unwrap()
            .contains("ErrEtcdMoveLeader")
    );
    assert_eq!(
        before_offline.entries,
        store.read(leader_key).await.unwrap().entries
    );
    assert_eq!(clients[next].status().await.unwrap().leader(), ids[next]);
    for node in &mut nodes {
        node.shutdown().await.unwrap();
    }
    drop(processes);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_045_transaction_failure_and_cancel() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let inner: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let id = inner.consensus_leader_id().await.unwrap().unwrap();
    let provider = Arc::new(
        EtcdMemberAdmin::connect(fixture.endpoint.clone(), id, Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let store = Arc::new(crate::member_admin_fault_test::MemberStore {
        inner: inner.clone(),
        mode: std::sync::atomic::AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .with_etcd_member_admin(provider)
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7451,
                member_id: 1,
                name: "member-1".into(),
                client_url: url.clone(),
                peer_url: "http://peer-1".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let read = Read::range(b"/pd/7451/leader", b"", Consistency::Linearizable);
    let before = inner.read(read.clone()).await.unwrap();
    store.mode.store(5, std::sync::atomic::Ordering::SeqCst);
    let failed = request(&url, "POST", "/pd/api/v1/leader/resign", "");
    assert_eq!(failed.0, 500);
    assert_eq!(before.entries, inner.read(read).await.unwrap().entries);
    assert!(
        inner
            .read(Read::range(
                b"/task045/oversized",
                b"",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    store.mode.store(4, std::sync::atomic::Ordering::SeqCst);
    let task =
        tokio::task::spawn_blocking(move || request(&url, "POST", "/pd/api/v1/leader/resign", ""));
    tokio::time::timeout(Duration::from_secs(5), store.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    let response = task.await.unwrap();
    assert!(matches!(response.0, 500 | 503), "{response:?}");
    node.shutdown().await.unwrap();
    assert_eq!(inner.consensus_leader_id().await.unwrap(), Some(id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_045_cancelled_recovery_is_typed() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = crate::cluster_restore::restore(
        astersql_pd_core::BasicCluster::new(),
        Arc::new(fixture.store().await),
        &NodeConfig {
            cluster_id: 7452,
            member_id: 1,
            name: "cancelled".into(),
            client_url: "http://127.0.0.1:2379".into(),
            peer_url: "http://127.0.0.1:2380".into(),
        },
        &cancel,
    )
    .await
    .err()
    .expect("cancelled recovery must fail");
    assert!(matches!(
        error.downcast_ref::<astersql_pd_member::MemberError>(),
        Some(astersql_pd_member::MemberError::Cancelled)
    ));
}

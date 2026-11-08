// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_storage::{Base, MetaStore};
use astersql_pd_storage_domain::StorageEndpoint;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_132_behavior() {
    let f = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let cancel = CancellationToken::new();
    let mut nodes = vec![];
    for member in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 132,
                    member_id: member,
                    name: format!("gc-{member}"),
                    client_url: url,
                    peer_url: format!("http://peer:{member}"),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
    }
    assert!(
        nodes[0].gc_manager().is_some(),
        "serving host must expose the canonical GC manager"
    );
    assert!(
        nodes[1].gc_manager().is_none(),
        "follower has no serving manager"
    );
    let old = nodes[0].gc_manager().unwrap();
    assert!(Arc::ptr_eq(&old, &nodes[0].gc_manager().unwrap()));
    let p = ClusterPaths::new(132);
    let ep = StorageEndpoint::new(store.clone());
    let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
    tx.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 100);
    tx.commit().await.unwrap();
    old.compatible_update_gc_safe_point(NULL_KEYSPACE_ID, 20)
        .await
        .unwrap();
    assert_eq!(
        old.get_gc_state(NULL_KEYSPACE_ID, true)
            .await
            .unwrap()
            .gc_safe_point,
        20
    );
    nodes[0].shutdown().await.unwrap();
    assert!(
        old.compatible_load_gc_safe_point(NULL_KEYSPACE_ID)
            .await
            .is_err(),
        "retired cache cannot serve"
    );
    assert!(
        old.compatible_update_gc_safe_point(NULL_KEYSPACE_ID, 30)
            .await
            .is_err()
    );
    let next = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Some(m) = nodes[1].gc_manager() {
                break m;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&old, &next));
    assert_eq!(
        next.compatible_load_gc_safe_point(NULL_KEYSPACE_ID)
            .await
            .unwrap(),
        20
    );
    next.compatible_update_gc_safe_point(NULL_KEYSPACE_ID, 40)
        .await
        .unwrap();
    assert!(
        old.compatible_update_gc_safe_point(NULL_KEYSPACE_ID, 50)
            .await
            .is_err()
    );
    assert_eq!(
        ep.load_gc_safe_point(&p, NULL_KEYSPACE_ID).await.unwrap(),
        40
    );
    nodes[1].shutdown().await.unwrap();
    assert!(
        next.compatible_load_gc_safe_point(NULL_KEYSPACE_ID)
            .await
            .is_err()
    );
    assert_eq!(
        Base::new(store.as_ref())
            .load(p.gc_safe_point_path(NULL_KEYSPACE_ID).as_bytes())
            .await
            .unwrap(),
        b"28"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_132_real_term_replacement_fences_prepared_raw_commit() {
    use astersql_pd_gc::GCStateManager;
    use astersql_pd_keyspace::KeyspaceManager;
    use astersql_pd_member::Leadership;
    let f = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let p = ClusterPaths::new(133);
    let cancel = CancellationToken::new();
    let leader = Arc::new(
        Leadership::campaign(
            store.clone(),
            cancel.clone(),
            133,
            b"same-value".to_vec(),
            10,
        )
        .await
        .unwrap(),
    );
    let bound: Arc<dyn MetaStore> = Arc::new(crate::member_role_worker::TermStore::new(
        store.clone(),
        leader.clone(),
        cancel.clone(),
    ));
    let m = GCStateManager::new(
        StorageEndpoint::new(bound.clone()),
        p.clone(),
        Arc::new(KeyspaceManager::new(
            astersql_pd_storage_endpoint::StorageEndpoint::new(bound).keyspace_storage(p.clone()),
        )),
        tokio::runtime::Handle::current(),
    );
    let role = m.on_node_becomes_leader_with_cancel(&cancel).await;
    let ep = StorageEndpoint::new(store.clone());
    let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
    tx.batch.set_txn_safe_point(NULL_KEYSPACE_ID, 100);
    tx.commit().await.unwrap();
    let pending = m
        .prepare_gc_safe_point(
            NULL_KEYSPACE_ID,
            30,
            astersql_pd_gc::GCSafePointMode::Compatible,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    // Same opaque value, new lease and revision: the old cancel token is deliberately still live.
    Base::new(store.as_ref())
        .remove(b"/pd/133/leader")
        .await
        .unwrap();
    let replacement = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        133,
        b"same-value".to_vec(),
        10,
    )
    .await
    .unwrap();
    assert!(pending.commit().await.is_err());
    assert_eq!(
        ep.load_gc_safe_point(&p, NULL_KEYSPACE_ID).await.unwrap(),
        0
    );
    role.shutdown().await;
    leader.resign().await.unwrap();
    replacement.validate().await.unwrap();
    replacement.resign().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_052_actual_host_term_owns_discovery_and_replacement() {
    let f = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let cancel = CancellationToken::new();
    let mut nodes = vec![];
    for member in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 520,
                    member_id: member,
                    name: format!("discovery-{member}"),
                    client_url: url,
                    peer_url: format!("http://peer:{member}"),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
    }
    let old = nodes[0].gc_manager().unwrap();
    assert!(old.enabled_keyspace_index().is_none());
    assert!(nodes[1].gc_manager().is_none());
    let old_index = old.start_metadata_discovery(2, 2).await.unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        old_index.snapshot_at_least(1, &cancel),
    )
    .await
    .unwrap()
    .unwrap();
    nodes[0].shutdown().await.unwrap();
    assert!(
        old_index
            .snapshot_at_least(0, &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(old.start_metadata_discovery(2, 2).await.is_err());
    let next = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Some(manager) = nodes[1].gc_manager() {
                break manager;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&old, &next));
    let next_index = next.start_metadata_discovery(2, 2).await.unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        next_index.snapshot_at_least(1, &cancel),
    )
    .await
    .unwrap()
    .unwrap();
    nodes[1].shutdown().await.unwrap();
    assert!(
        next_index
            .snapshot_at_least(0, &CancellationToken::new())
            .await
            .is_err()
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_053_host_metrics_lifetime() {
    use astersql_pd_storage_domain::BarrierTime;
    let f = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let registry = Arc::new(prometheus::Registry::new());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            NodeConfig {
                cluster_id: 530,
                member_id: 1,
                name: "gc-metrics".into(),
                client_url: url.clone(),
                peer_url: "http://peer:1".into(),
            },
            listener,
            registry.clone(),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let manager = node.gc_manager().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let ts = ((now.as_secs() - 172800) * 1000) << 18;
    let time = BarrierTime::from_unix(now.as_secs() as i64, 0);
    manager
        .set_gc_barrier(
            NULL_KEYSPACE_ID,
            "host",
            ts,
            "2562047h47m16.854775807s".parse().unwrap(),
            time,
        )
        .await
        .unwrap();
    manager
        .advance_txn_safe_point(NULL_KEYSPACE_ID, ts + 100, time)
        .await
        .unwrap();
    let output = tokio::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--max-time",
            "5",
            &format!("{url}/metrics"),
        ])
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("pd_gc_barrier_timestamp_seconds{barrier_id=\"host\""),
        "{text}"
    );
    node.shutdown().await.unwrap();
    assert!(
        registry
            .gather()
            .iter()
            .all(|f| f.name() != "pd_gc_barrier_timestamp_seconds")
    );
    for sample in text
        .lines()
        .filter(|line| line.contains("pd_gc_barrier_timestamp_seconds"))
    {
        println!("{sample}");
    }
    println!("pd_gc_barrier_timestamp_seconds absent after shutdown");
}

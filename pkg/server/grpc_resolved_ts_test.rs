// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[path = "grpc_resolved_ts_scenarios_test.rs"]
mod scenarios;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_034_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 2700,
                member_id: 1,
                name: "report".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let result = client
        .report_min_resolved_ts(pdpb::ReportMinResolvedTsRequest {
            header: scenarios::header(2700),
            store_id: 1,
            min_resolved_ts: 42,
        })
        .await;
    assert!(
        result.is_ok(),
        "ReportMinResolvedTS must serve protocol responses: {result:?}"
    );
    assert_eq!(
        result
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .unwrap()
            .r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    scenarios::admission(&mut client, 2700).await;
    super::grpc_region_lookup_scenarios_test::prepare(&mut client, 2700).await;
    scenarios::observe(&mut client, 2700).await;
    assert!(
        std::process::Command::new("curl")
            .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
            .output()
            .unwrap()
            .status
            .success()
    );
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    let storage = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(2700);
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while storage.load_min_resolved_ts(&paths).await.unwrap() != 100 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    scenarios::report(&mut client, 2700, 1, 7).await.unwrap();

    assert_eq!(node.cluster().min_resolved_ts(), 100);
    assert_eq!(storage.load_min_resolved_ts(&paths).await.unwrap(), 100);
    let host = node.cluster();
    cancel.cancel();
    assert!(scenarios::report(&mut client, 2700, 1, 999).await.is_err());
    assert!(host.report_min_resolved_ts(1, 999).await.is_err());
    node.shutdown().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn task_034_monotonic_worker_and_cancel() {
    use astersql_pd_cluster::{ResolvedTsWorker, refresh_min_resolved_ts};
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    let current = Arc::new(AtomicU64::new(100));
    let candidate = Arc::new(AtomicU64::new(7));
    let storage = Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default())));
    let paths = ClusterPaths::new(3400);
    storage.save_min_resolved_ts(&paths, 100).await.unwrap();
    let (c, t, s) = (current.clone(), candidate.clone(), storage.clone());
    let worker = ResolvedTsWorker::start(
        &tokio::runtime::Handle::current(),
        &CancellationToken::new(),
        || Duration::from_secs(1),
        move || {
            let (c, t, s) = (c.clone(), t.clone(), s.clone());
            async move {
                refresh_min_resolved_ts(Some(t.load(Ordering::Relaxed)), &c, &s, &paths)
                    .await
                    .unwrap();
            }
        },
    );
    tokio::task::yield_now().await;
    for value in [7, 0, u64::MAX, 100, 200] {
        candidate.store(value, Ordering::Relaxed);
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        let expected = if value == 200 { 200 } else { 100 };
        assert_eq!(current.load(Ordering::Relaxed), expected);
        assert_eq!(
            storage.load_min_resolved_ts(&paths).await.unwrap(),
            expected
        );
    }
    worker.stop().await.unwrap();
    candidate.store(300, Ordering::Relaxed);
    tokio::time::advance(Duration::from_secs(10)).await;
    assert_eq!(storage.load_min_resolved_ts(&paths).await.unwrap(), 200);
    assert_eq!(Arc::strong_count(&current), 1);
}

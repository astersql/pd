// Copyright 2026 AsterSQL.

use std::{future::Future, time::Duration};

use prometheus::Registry;
use tokio::{runtime::Handle, time::timeout};
use tokio_util::sync::CancellationToken;

use crate::*;

fn report(id: u64, keys: &[&str]) -> BucketReport {
    let zero = vec![0; keys.len() - 1];
    BucketReport {
        region_id: id,
        version: 1,
        keys: keys.iter().map(|s| s.as_bytes().to_vec()).collect(),
        period_in_ms: 1000,
        flows: BucketFlows {
            read_bytes: zero.clone(),
            read_keys: zero.clone(),
            read_qps: zero.clone(),
            write_bytes: zero.clone(),
            write_keys: zero.clone(),
            write_qps: zero,
        },
    }
}
fn start(index: BucketRangeIndex) -> (BucketWorker, CancellationToken, Registry) {
    let parent = CancellationToken::new();
    let registry = Registry::new();
    let worker = BucketWorker::start(&Handle::current(), &parent, &registry, index).unwrap();
    (worker, parent, registry)
}
async fn collect(client: &BucketWorkerClient, min: i32, ids: Vec<u64>) -> BucketStats {
    timeout(
        Duration::from_secs(5),
        client
            .collect(min, ids)
            .unwrap()
            .wait(&CancellationToken::new()),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn state_task_08_bucket_worker_go_check_fifo() {
    let (mut worker, parent, registry) = start(BucketRangeIndex::new());
    let client = worker.client();
    client.check_async(report(1, &["10", "20", "30"])).unwrap();
    let first = client.collect(-20, vec![]).unwrap();
    client.check_async(report(2, &["20", "30"])).unwrap();
    let second = client.collect(-20, vec![]).unwrap();
    client.check_async(report(1, &["10", "20"])).unwrap();
    let third = client.collect(-20, vec![]).unwrap();
    let cancel = CancellationToken::new();
    let first = first.wait(&cancel).await.unwrap();
    let second = second.wait(&cancel).await.unwrap();
    let third = third.wait(&cancel).await.unwrap();
    assert_eq!(
        first[&1].iter().map(|b| b.hot_degree).collect::<Vec<_>>(),
        [-1, -1]
    );
    assert_eq!(second.len(), 1);
    assert_eq!(second[&2][0].hot_degree, -2);
    assert_eq!(third.len(), 2);
    assert_eq!(third[&1][0].hot_degree, -2);
    assert_eq!(third[&2][0].hot_degree, -2);
    assert_eq!(first[&1][1].loads, [0; 6]);
    assert_eq!(first[&1][1].interval, 1000);
    assert_eq!(first[&1][1].start_key, b"20");
    assert_eq!(first[&1][1].end_key, b"30");
    let families = registry.gather();
    let metric = families
        .iter()
        .find(|m| m.name() == "pd_scheduler_bucket_task_duration")
        .unwrap();
    assert_eq!(
        metric.help(),
        "Bucketed histogram of processing time (s) of bucket task."
    );
    assert_eq!(metric.get_metric().len(), 2);
    for m in metric.get_metric() {
        assert_eq!(m.get_label()[0].name(), "type");
        assert!(["check_buckets", "collect_bucket_stats"].contains(&m.get_label()[0].value()));
        assert_eq!(m.get_histogram().get_sample_count(), 3);
        assert_eq!(m.get_histogram().get_bucket().len(), 23);
        for (i, b) in m.get_histogram().get_bucket().iter().enumerate() {
            assert_eq!(b.upper_bound(), 0.0001 * 2_f64.powi(i as i32));
        }
    }
    timeout(Duration::from_secs(5), worker.shutdown())
        .await
        .unwrap()
        .unwrap();
    worker.shutdown().await.unwrap();
    assert!(!parent.is_cancelled());
    assert_eq!(
        client.check_async(report(3, &["30", "40"])),
        Err(BucketSubmitError::Closed)
    );
}

#[tokio::test]
async fn state_task_08_bucket_worker_go_collect_snapshot() {
    let mut index = BucketRangeIndex::new();
    for i in 0..10 {
        index.put_item(convert_to_bucket_tree_item(&report(
            i,
            &[&(i * 10).to_string(), &((i + 1) * 10).to_string()],
        )));
    }
    let (mut worker, _, _) = start(index);
    let client = worker.client();
    let before = collect(&client, -100, vec![]).await;
    assert_eq!(before.len(), 10);
    assert!(collect(&client, 1, vec![]).await.is_empty());
    assert_eq!(collect(&client, 0, vec![3, 99, 3]).await.len(), 1);
    client.check_async(report(3, &["30", "40"])).unwrap();
    let after = collect(&client, -20, vec![3]).await;
    assert_eq!(after[&3][0].hot_degree, -1);
    assert_eq!(before[&3][0].hot_degree, 0);
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_08_bucket_worker_full_and_pending_shutdown() {
    // Current-thread runtime: no await until the queue is filled; the spawned
    // consumer cannot run. This controls scheduling, not domain results.
    let (mut worker, _, registry) = start(BucketRangeIndex::new());
    let client = worker.client();
    let pending = client.collect(-20, vec![]).unwrap();
    for _ in 1..BUCKET_TASK_QUEUE_CAPACITY {
        client.check_async(report(1, &["10", "20"])).unwrap();
    }
    assert_eq!(
        client.check_async(report(2, &["20", "30"])),
        Err(BucketSubmitError::Full)
    );
    assert!(matches!(
        client.collect(0, vec![]),
        Err(BucketSubmitError::Full)
    ));
    timeout(Duration::from_secs(5), worker.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        pending.wait(&CancellationToken::new()).await,
        Err(BucketWaitError::WorkerStopped)
    );
    assert!(
        registry
            .gather()
            .iter()
            .flat_map(|f| f.get_metric())
            .all(|m| m.get_histogram().get_sample_count() == 0)
    ); // No queued task executed on shutdown.
}

#[tokio::test]
async fn state_task_08_bucket_worker_cancel_wait_and_drop_reply() {
    let (mut worker, _, registry) = start(BucketRangeIndex::new());
    let client = worker.client();
    let wait = client.collect(-20, vec![]).unwrap();
    let cancel = CancellationToken::new();
    let cancel_later = cancel.clone();
    // Poll the wait to Pending before cancellation, without giving the worker
    // a chance to send. This proves cancellation during a suspended wait.
    let wait = wait.wait(&cancel);
    tokio::pin!(wait);
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(wait.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    cancel_later.cancel();
    assert_eq!(wait.await, Err(BucketWaitError::Cancelled));
    drop(client.collect(-20, vec![]).unwrap());
    client.check_async(report(1, &["10", "20"])).unwrap();
    let result = collect(&client, -20, vec![]).await;
    assert_eq!(result[&1][0].hot_degree, -1);
    let families = registry.gather();
    let query = families[0]
        .get_metric()
        .iter()
        .find(|m| m.get_label()[0].value() == "collect_bucket_stats")
        .unwrap();
    assert_eq!(query.get_histogram().get_sample_count(), 3);
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_08_bucket_worker_parent_cancel_and_drop() {
    let (mut worker, parent, _) = start(BucketRangeIndex::new());
    let client = worker.client();
    let pending = client.collect(-20, vec![]).unwrap();
    parent.cancel();
    assert_eq!(
        pending.wait(&CancellationToken::new()).await,
        Err(BucketWaitError::WorkerStopped)
    );
    worker.shutdown().await.unwrap();
    assert!(matches!(
        client.collect(0, vec![]),
        Err(BucketSubmitError::Closed)
    ));
    let (mut worker, parent, _) = start(BucketRangeIndex::new());
    let client = worker.client();
    let wait = client.collect(0, vec![]).unwrap();
    // Retain the task handle in the test so drop cancellation is also joined.
    let join = worker.join_for_test();
    drop(worker);
    assert_eq!(
        wait.wait(&CancellationToken::new()).await,
        Err(BucketWaitError::WorkerStopped)
    );
    timeout(Duration::from_secs(5), join)
        .await
        .unwrap()
        .unwrap();
    assert!(!parent.is_cancelled());
}

#[tokio::test]
async fn state_task_08_bucket_worker_duplicate_registry_no_spawn() {
    let registry = Registry::new();
    let parent = CancellationToken::new();
    let mut worker = BucketWorker::start(
        &Handle::current(),
        &parent,
        &registry,
        BucketRangeIndex::new(),
    )
    .unwrap();
    assert!(
        BucketWorker::start(
            &Handle::current(),
            &parent,
            &registry,
            BucketRangeIndex::new()
        )
        .is_err()
    );
    assert!(!parent.is_cancelled());
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn state_task_08_bucket_worker_shutdown_wait_is_cancel_safe() {
    let (mut worker, parent, _) = start(BucketRangeIndex::new());
    let client = worker.client();
    let pending = client.collect(0, vec![]).unwrap();
    {
        let shutdown = worker.shutdown();
        tokio::pin!(shutdown);
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(shutdown.as_mut().poll(cx)))
                .await
                .is_pending()
        );
        // Dropping this suspended shutdown future must retain the join handle.
    }
    timeout(Duration::from_secs(5), worker.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        pending.wait(&CancellationToken::new()).await,
        Err(BucketWaitError::WorkerStopped)
    );
    assert!(!parent.is_cancelled());
}

#[tokio::test]
async fn state_task_08_bucket_worker_panic_propagates_and_unblocks() {
    let (mut worker, _, _) = start(BucketRangeIndex::new());
    let client = worker.client();
    // Match the existing converter's programmer-error boundary, not a mocked result.
    client.check_async(BucketReport::default()).unwrap();
    let pending = client.collect(0, vec![]).unwrap();
    assert_eq!(
        pending.wait(&CancellationToken::new()).await,
        Err(BucketWaitError::WorkerStopped)
    );
    let error = timeout(Duration::from_secs(5), worker.shutdown())
        .await
        .unwrap()
        .unwrap_err();
    assert!(error.is_panic());
    assert!(matches!(
        client.collect(0, vec![]),
        Err(BucketSubmitError::Closed)
    ));
    worker.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn state_task_08_bucket_worker_shared_host_concurrent_producers() {
    let (mut worker, parent, _) = start(BucketRangeIndex::new());
    let client = worker.client();
    let mut producers = Vec::new();
    for id in 1..=4 {
        let client = client.clone();
        producers.push(tokio::spawn(async move {
            for _ in 0..10 {
                client
                    .check_async(report(id, &[&format!("{id}0"), &format!("{id}9")]))
                    .unwrap();
                tokio::task::yield_now().await;
            }
        }));
    }
    for producer in producers {
        producer.await.unwrap();
    }
    let result = collect(&client, -20, vec![]).await;
    assert_eq!(result.len(), 4);
    for id in 1..=4 {
        assert_eq!(result[&id][0].hot_degree, -10);
    }
    worker.shutdown().await.unwrap();
    assert!(!parent.is_cancelled());
    // Same host still runs unrelated work after the worker exits.
    assert_eq!(tokio::spawn(async { 42 }).await.unwrap(), 42);
}

#[tokio::test]
async fn state_task_08_bucket_worker_full_rejection_has_no_effect() {
    let (mut worker, _, _) = start(BucketRangeIndex::new());
    let client = worker.client();
    for _ in 0..BUCKET_TASK_QUEUE_CAPACITY {
        client.check_async(report(1, &["10", "20"])).unwrap();
    }
    assert_eq!(
        client.check_async(report(2, &["20", "30"])),
        Err(BucketSubmitError::Full)
    );
    let wait = timeout(Duration::from_secs(5), async {
        loop {
            match client.collect(-20, vec![]) {
                Ok(wait) => break wait,
                Err(BucketSubmitError::Full) => tokio::task::yield_now().await,
                Err(err) => panic!("unexpected submission error: {err}"),
            }
        }
    })
    .await
    .unwrap();
    let stats = wait.wait(&CancellationToken::new()).await.unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[&1][0].hot_degree, -20);
    worker.shutdown().await.unwrap();
}

// Copyright 2026 AsterSQL.
use crate::{HeartbeatCommand, Operation, routing::*, stream_lifecycle::*};
use astersql_pd_core::{NodeState, Peer, StoreInfo, StoreMeta, StoresInfo};
use prometheus::Registry;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

fn command(id: u64) -> HeartbeatCommand {
    HeartbeatCommand {
        cluster_id: 123,
        region_id: 55,
        region_epoch: None,
        target_peer: Peer {
            id: 1,
            store_id: id,
            ..Peer::default()
        },
        operation: Operation::default(),
    }
}
fn stores() -> Arc<StoresInfo> {
    let s = Arc::new(StoresInfo::new());
    for id in 1..=6 {
        s.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            address: format!("mock://tikv-{id}:{id}"),
            ..StoreMeta::default()
        })));
    }
    s
}
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}
fn rows(registry: &Registry) -> Value {
    let mut rows = vec![];
    for family in registry.gather() {
        for m in family.get_metric() {
            let labels: std::collections::HashMap<_, _> = m
                .get_label()
                .iter()
                .map(|l| (l.name(), l.value()))
                .collect();
            rows.push(json!({"address":labels["address"],"store":labels["store"],"kind":labels["type"],"status":labels["status"],"value":m.get_counter().get_value() as u64}));
        }
    }
    rows.sort_by_key(|r| {
        (
            r["store"].as_str().unwrap().to_owned(),
            r["kind"].as_str().unwrap().to_owned(),
            r["status"].as_str().unwrap().to_owned(),
        )
    });
    json!(rows)
}
#[tokio::test(start_paused = true)]
async fn task_014_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_014.json"
    ))
    .unwrap();
    let registry = Registry::new();
    let metrics = Arc::new(HeartbeatMetrics::new(&registry).unwrap());
    let stores = stores();
    let (handle, router) = StreamRouter::new(stores.clone());
    let mut worker = HeartbeatStreams::start(
        router,
        123,
        metrics.clone(),
        &tokio::runtime::Handle::current(),
    );
    let (good, mut rx) = mpsc::channel::<HeartbeatMessage>(2048);
    let (bad, bad_rx) = mpsc::channel::<HeartbeatMessage>(1);
    drop(bad_rx);
    let cases = fixture["cases"].as_array().unwrap();

    handle.send(command(1)).await.unwrap();
    settle().await;
    assert_eq!(rows(&registry), cases[0]["metrics"]);
    let gathered = registry.gather();
    let family = &gathered[0];
    assert!(
        fixture["descriptor"]
            .as_str()
            .unwrap()
            .contains(family.name())
    );
    assert!(
        fixture["descriptor"]
            .as_str()
            .unwrap()
            .contains(family.help())
    );
    assert_eq!(format!("{:?}", family.get_field_type()), "COUNTER");
    handle
        .bind(1, Arc::new(MessageStream(good.clone())))
        .await
        .unwrap();
    settle().await;
    handle.send(command(1)).await.unwrap();
    settle().await;
    assert_eq!(
        rx.recv().await.unwrap(),
        HeartbeatMessage::Command(command(1))
    );
    assert_eq!(rows(&registry), cases[1]["metrics"]);
    handle
        .bind(1, Arc::new(MessageStream(bad.clone())))
        .await
        .unwrap();
    settle().await;
    handle.send(command(1)).await.unwrap();
    settle().await;
    assert_eq!(rows(&registry), cases[2]["metrics"]);
    for (id, tx) in [(1, good.clone()), (2, bad), (99, good.clone()), (3, good)] {
        handle.bind(id, Arc::new(MessageStream(tx))).await.unwrap();
        settle().await;
    }
    stores.patch_store(3, |s| s.edit_meta(|m| m.node_state = NodeState::Removed));
    tokio::time::advance(Duration::from_secs(59)).await;
    settle().await;
    assert!(rx.try_recv().is_err(), "no immediate or early tick");
    tokio::time::advance(Duration::from_secs(1)).await;
    settle().await;
    let message = rx.recv().await.unwrap();
    assert_eq!(
        message,
        HeartbeatMessage::KeepAlive {
            cluster_id: fixture["keepalive"]["header"]["cluster_id"]
                .as_u64()
                .unwrap()
        }
    );
    assert_eq!(fixture["keepalive"], json!({"header":{"cluster_id":123}}));
    assert_eq!(rows(&registry), cases[3]["metrics"]);
    worker.stop();
    worker.stop();
    worker.join().await.unwrap();
    worker.join().await.unwrap();
    tokio::time::advance(Duration::from_secs(180)).await;
    settle().await;
    assert!(rx.try_recv().is_err());
    assert!(handle.send(command(1)).await.is_err());
    assert_eq!(rows(&registry), cases[4]["metrics"]);
    metrics.delete_store_metrics(1);
    assert!(
        rows(&registry)
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["store"] != "1")
    );
    metrics.reset_metrics();
    assert_eq!(rows(&registry), json!([]));
    println!("task_014 Go oracle: 5 lifecycle/metrics cases + 1 saturated close case");
}

#[tokio::test(start_paused = true)]
async fn task_014_saturated_cancel_and_join() {
    let registry = Registry::new();
    let metrics = Arc::new(HeartbeatMetrics::new(&registry).unwrap());
    let stores = stores();
    let (handle, mut router) = StreamRouter::new(stores.clone());
    let (tx, mut rx) = mpsc::channel::<HeartbeatMessage>(1);
    tx.send(HeartbeatMessage::Command(command(1)))
        .await
        .unwrap();
    handle
        .bind(1, Arc::new(MessageStream(tx.clone())))
        .await
        .unwrap();
    router.step().await.unwrap();
    handle.send(command(1)).await.unwrap();
    let mut worker =
        HeartbeatStreams::start(router, 123, metrics, &tokio::runtime::Handle::current());
    settle().await;
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_014.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["saturated_queue"]["before"],
        HEARTBEAT_CHANNEL_CAPACITY
    );
    for _ in 0..fixture["saturated_queue"]["before"].as_u64().unwrap() {
        handle.try_send(command(1)).unwrap();
    }
    assert_eq!(
        fixture["saturated_queue"]["after"],
        HEARTBEAT_CHANNEL_CAPACITY
    );
    assert_eq!(
        fixture["saturated_queue"]["send_after_close_returned"],
        true
    );
    let mut pending = Box::pin(handle.send(command(1)));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(pending.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    handle
        .bind(2, Arc::new(MessageStream(tx.clone())))
        .await
        .unwrap();
    let mut bind = Box::pin(handle.bind(3, Arc::new(MessageStream(tx))));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(bind.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    worker.stop();
    worker.stop();
    assert!(pending.await.is_err());
    assert!(bind.await.is_err());
    let mut join = Box::pin(worker.join());
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(join.as_mut().poll(cx)))
            .await
            .is_pending(),
        "join waits for in-flight external I/O, as Go Close"
    );
    stores.patch_store(1, |s| s.edit_meta(|m| m.node_state = NodeState::Removed));
    rx.recv().await.unwrap();
    join.await.unwrap();
    worker.join().await.unwrap();
    assert_eq!(
        rx.recv().await.unwrap(),
        HeartbeatMessage::Command(command(1))
    );
    assert!(
        rx.try_recv().is_err(),
        "queued commands must not drain after cancellation"
    );
    assert_eq!(
        rows(&registry),
        json!([]),
        "no metrics recreated after store removal during send"
    );
    assert!(handle.try_send(command(1)).is_err());
}

#[tokio::test(start_paused = true)]
async fn task_014_drop_and_keepalive_failure() {
    let registry = Registry::new();
    let metrics = Arc::new(HeartbeatMetrics::new(&registry).unwrap());
    let (handle, mut router) = StreamRouter::new(stores());
    let (tx, mut rx) = mpsc::channel::<HeartbeatMessage>(1);
    handle.bind(1, Arc::new(MessageStream(tx))).await.unwrap();
    router.step().await.unwrap();
    let worker = HeartbeatStreams::start(router, 123, metrics, &tokio::runtime::Handle::current());
    settle().await;
    tokio::time::advance(Duration::from_secs(60)).await;
    settle().await;
    assert_eq!(
        rx.recv().await.unwrap(),
        HeartbeatMessage::KeepAlive { cluster_id: 123 }
    );
    drop(rx);
    tokio::time::advance(Duration::from_secs(60)).await;
    settle().await;
    let r = rows(&registry);
    assert_eq!(r.as_array().unwrap().len(), 2);
    assert!(r.as_array().unwrap().iter().all(|r| r["value"] == 1));
    tokio::time::advance(Duration::from_secs(60)).await;
    settle().await;
    assert_eq!(
        rows(&registry),
        r,
        "failed keepalive removes the stream and never retries"
    );
    drop(worker);
    settle().await;
    assert!(handle.send(command(1)).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn task_014_keepalive_join_and_metric_registry() {
    let registry = Registry::new();
    let metrics = Arc::new(HeartbeatMetrics::new(&registry).unwrap());
    assert!(HeartbeatMetrics::new(&registry).is_err());
    assert!(HeartbeatMetrics::new(&Registry::new()).is_ok());
    let stores = stores();
    let (handle, mut router) = StreamRouter::new(stores.clone());
    let (tx, mut rx) = mpsc::channel::<HeartbeatMessage>(1);
    tx.send(HeartbeatMessage::Command(command(1)))
        .await
        .unwrap();
    handle.bind(1, Arc::new(MessageStream(tx))).await.unwrap();
    router.step().await.unwrap();
    let mut worker = HeartbeatStreams::start(
        router,
        123,
        metrics.clone(),
        &tokio::runtime::Handle::current(),
    );
    settle().await;
    tokio::time::advance(Duration::from_secs(60)).await;
    settle().await;
    worker.stop();
    let mut join = Box::pin(worker.join());
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(join.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    // Dropping a pending join must retain the worker handle for the next join.
    drop(join);
    stores.patch_store(1, |s| s.edit_meta(|m| m.node_state = NodeState::Removed));
    metrics.delete_store_metrics(1);
    rx.recv().await.unwrap();
    worker.join().await.unwrap();
    assert_eq!(
        rx.recv().await.unwrap(),
        HeartbeatMessage::KeepAlive { cluster_id: 123 }
    );
    assert_eq!(rows(&registry), json!([]));
    assert!(handle.send(command(1)).await.is_err());
}

#[tokio::test]
async fn task_014_router_oracle_bindings() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_014.json"
    ))
    .unwrap();
    let registry = Registry::new();
    let metrics = Arc::new(HeartbeatMetrics::new(&registry).unwrap());
    let stores = stores();
    let (handle, mut router) = StreamRouter::new(stores.clone());
    router.set_metrics(metrics);
    let (good, mut rx) = mpsc::channel::<HeartbeatMessage>(1024);
    let (bad, bad_rx) = mpsc::channel::<HeartbeatMessage>(1);
    drop(bad_rx);
    let mut delivered = 0;
    for (i, case) in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .take(4)
        .enumerate()
    {
        if i == 1 || i == 2 {
            let tx = if i == 1 { good.clone() } else { bad.clone() };
            handle.bind(1, Arc::new(MessageStream(tx))).await.unwrap();
            router.step().await.unwrap();
        }
        if i < 3 {
            handle.send(command(1)).await.unwrap();
            router.step().await.unwrap();
        } else {
            stores.patch_store(3, |s| s.edit_meta(|m| m.node_state = NodeState::Removed));
            for (id, tx) in [
                (1, good.clone()),
                (2, bad.clone()),
                (99, good.clone()),
                (3, good.clone()),
            ] {
                handle.bind(id, Arc::new(MessageStream(tx))).await.unwrap();
                router.step().await.unwrap();
            }
            router.keep_alive(123).await;
        }
        while let Ok(msg) = rx.try_recv() {
            delivered += 1;
            assert_eq!(
                msg,
                if i == 3 {
                    HeartbeatMessage::KeepAlive { cluster_id: 123 }
                } else {
                    HeartbeatMessage::Command(command(1))
                }
            );
        }
        assert_eq!(json!(router.bound_stores()), case["bound_stores"]);
        assert_eq!(json!(delivered), case["delivered"]);
        assert_eq!(rows(&registry), case["metrics"]);
    }
}

// Copyright 2026 AsterSQL.
use crate::{HeartbeatCommand, Operation, routing::*};
use astersql_pd_core::{NodeState, Peer, StoreInfo, StoreMeta, StoresInfo};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::mpsc;

fn command(region_id: u64, store_id: u64) -> HeartbeatCommand {
    HeartbeatCommand {
        cluster_id: 123,
        region_id,
        region_epoch: None,
        target_peer: Peer {
            id: 1,
            store_id,
            ..Peer::default()
        },
        operation: Operation::default(),
    }
}
fn stores() -> Arc<StoresInfo> {
    let stores = Arc::new(StoresInfo::new());
    for id in 1..=6 {
        stores.put_store(Arc::new(StoreInfo::new(StoreMeta {
            id,
            ..StoreMeta::default()
        })));
    }
    stores
}

#[tokio::test]
async fn task_013_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_013.json"
    ))
    .unwrap();
    assert_eq!(
        HEARTBEAT_CHANNEL_CAPACITY as u64,
        fixture["capacities"]["msgCh"].as_u64().unwrap()
    );
    assert_eq!(fixture["capacities"]["streamCh"], 1);
    let stores = stores();
    let (handle, mut router) = StreamRouter::new(stores.clone());
    let (tx1, mut rx1) = mpsc::channel(1024);
    let (tx2, mut rx2) = mpsc::channel(1024);
    let mut history = [Vec::new(), Vec::new()];
    for case in fixture["cases"].as_array().unwrap() {
        for op in case["operations"].as_array().unwrap() {
            match op[0].as_str().unwrap() {
                "bind" => {
                    let tx = if op[2] == 1 { tx1.clone() } else { tx2.clone() };
                    handle
                        .bind(op[1].as_u64().unwrap(), Arc::new(tx))
                        .await
                        .unwrap();
                    assert!(matches!(router.step().await, Some(RouteEvent::Bound(_))));
                }
                "send" => {
                    handle
                        .send(command(op[1].as_u64().unwrap(), op[2].as_u64().unwrap()))
                        .await
                        .unwrap();
                    assert!(router.step().await.is_some());
                }
                "remove" => {
                    stores.patch_store(op[1].as_u64().unwrap(), |store| {
                        store.edit_meta(|meta| meta.node_state = NodeState::Removed)
                    });
                }
                "disconnect" => rx1.close(),
                other => panic!("unknown oracle operation {other}"),
            }
        }
        for (i, rx) in [&mut rx1, &mut rx2].into_iter().enumerate() {
            while let Ok(message) = rx.try_recv() {
                history[i].push(message);
            }
        }
        assert_eq!(
            json!({"stream1": history[0].len(), "stream2": history[1].len()}),
            case["queue_lengths"],
            "{}",
            case["id"]
        );
        assert_eq!(
            json!(router.bound_stores()),
            case["bound_stores"],
            "{}",
            case["id"]
        );
        // Oracle payloads are from the stream with newly delivered messages.
        for expected in case["messages"].as_array().unwrap() {
            let actual = history
                .iter()
                .flatten()
                .find(|m| m.region_id == expected["region_id"].as_u64().unwrap())
                .unwrap();
            assert_eq!(
                actual.cluster_id,
                expected["header"]["cluster_id"].as_u64().unwrap()
            );
            assert_eq!(
                actual.target_peer.id,
                expected["target_peer"]["id"].as_u64().unwrap()
            );
            assert_eq!(
                actual.target_peer.store_id,
                expected["target_peer"]["store_id"].as_u64().unwrap()
            );
            assert_eq!(
                actual,
                &command(actual.region_id, actual.target_peer.store_id)
            );
        }
    }
    assert_eq!(
        history[1].iter().map(|m| m.region_id).collect::<Vec<_>>(),
        [56, 60, 61, 62, 72]
    );
    println!("task_013 oracle: 9 cases passed, 0 failed");
}

#[tokio::test]
async fn task_013_bounded_queue_order_and_errors() {
    let (handle, mut router) = StreamRouter::new(stores());
    let (tx, mut rx) = mpsc::channel(1024);
    handle.bind(1, Arc::new(tx)).await.unwrap();
    assert!(matches!(router.step().await, Some(RouteEvent::Bound(1))));
    for id in 0..HEARTBEAT_CHANNEL_CAPACITY {
        handle.try_send(command(id as u64, 1)).unwrap();
    }
    assert!(matches!(
        handle.try_send(command(1024, 1)),
        Err(mpsc::error::TrySendError::Full(_))
    ));
    for id in 0..HEARTBEAT_CHANNEL_CAPACITY {
        assert!(matches!(router.step().await, Some(RouteEvent::Sent(1))));
        assert_eq!(rx.recv().await.unwrap().region_id, id as u64);
    }
    drop(rx);
    handle.send(command(2000, 1)).await.unwrap();
    let Some(RouteEvent::SendFailed { store_id, error }) = router.step().await else {
        panic!("expected transport failure")
    };
    assert_eq!(store_id, 1);
    assert_eq!(error.to_string(), "channel closed");
    assert!(router.bound_stores().is_empty());
    handle.send(command(2001, 1)).await.unwrap();
    assert!(matches!(router.step().await, Some(RouteEvent::Unbound(1))));
    drop(handle);
    assert!(router.step().await.is_none());
}

#[tokio::test]
async fn task_013_rebind_waits_for_inflight_send() {
    let stores = stores();
    let (handle, mut router) = StreamRouter::new(stores.clone());
    let (tx1, mut rx1) = mpsc::channel(1);
    let (tx2, mut rx2) = mpsc::channel(1);
    tx1.send(command(1, 1)).await.unwrap();
    handle.bind(1, Arc::new(tx1)).await.unwrap();
    assert!(matches!(router.step().await, Some(RouteEvent::Bound(1))));
    handle.send(command(2, 1)).await.unwrap();
    // Poll the real send to Pending because the old transport is full.
    let mut send = Box::pin(router.step());
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(send.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    handle.bind(1, Arc::new(tx2)).await.unwrap();
    // A second update must wait for the single update slot.
    let (tx3, mut rx3) = mpsc::channel(1);
    let mut bind = Box::pin(handle.bind(2, Arc::new(tx3)));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(bind.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    stores.patch_store(1, |s| s.edit_meta(|m| m.node_state = NodeState::Removed));
    assert_eq!(rx1.recv().await.unwrap().region_id, 1);
    assert!(matches!(send.await, Some(RouteEvent::Sent(1))));
    assert_eq!(rx1.recv().await.unwrap().region_id, 2);
    // Go serializes rebinding behind Send; removal while blocked does not undo a successful send.
    assert!(matches!(router.step().await, Some(RouteEvent::Bound(1))));
    bind.await.unwrap();
    assert!(matches!(router.step().await, Some(RouteEvent::Bound(2))));
    handle.send(command(3, 1)).await.unwrap();
    assert!(matches!(
        router.step().await,
        Some(RouteEvent::RemovedStore(1))
    ));
    assert!(rx2.try_recv().is_err());
    handle.send(command(4, 2)).await.unwrap();
    assert!(matches!(router.step().await, Some(RouteEvent::Sent(2))));
    assert_eq!(rx3.recv().await.unwrap().region_id, 4);
    assert!(rx1.try_recv().is_err());
    drop(router);
    assert!(handle.send(command(5, 2)).await.is_err());
    assert!(matches!(
        handle.try_send(command(6, 2)),
        Err(mpsc::error::TrySendError::Closed(_))
    ));
}

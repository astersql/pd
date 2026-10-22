// Copyright 2026 AsterSQL.
use crate::{Limiter, UpdateStatus};

#[tokio::test(start_paused = true)]
async fn limiter_config_updates() {
    let l = Limiter::new();
    assert_eq!(UpdateStatus::NoChange as u32, 2);
    assert_eq!(UpdateStatus::Updated as u32, 4);
    assert_eq!(UpdateStatus::Deleted as u32, 8);
    assert_eq!(l.concurrency_status().await, (0, 0));
    assert_eq!(l.qps_status().await, (0.0, 0));
    assert_eq!(l.update_concurrency_config(0).await, UpdateStatus::NoChange);
    assert_eq!(l.update_qps_config(0.0, 0).await, UpdateStatus::NoChange);
    assert_eq!(l.update_concurrency_config(10).await, UpdateStatus::Updated);
    let old = l.concurrency_limiter().await;
    assert!(old.allow());
    assert_eq!(l.update_concurrency_config(5).await, UpdateStatus::Updated);
    assert_eq!(l.concurrency_status().await, (5, 1));
    assert_eq!(l.update_concurrency_config(5).await, UpdateStatus::NoChange);
    assert_eq!(l.update_concurrency_config(0).await, UpdateStatus::Deleted);
    assert_eq!(l.concurrency_status().await, (0, 0));
    old.release();
    assert_eq!(l.concurrency_status().await, (0, 0));
    assert_eq!(l.update_qps_config(1.0, 1).await, UpdateStatus::Updated);
    let bucket = l.rate_limiter().await.unwrap();
    assert!(bucket.allow().await);
    assert_eq!(
        l.update_qps_config(1.0 + 0.5e-8, 1).await,
        UpdateStatus::NoChange
    );
    assert_eq!(l.qps_status().await, (1.0, 1));
    assert_eq!(l.update_qps_config(5.0, 5).await, UpdateStatus::Updated);
    assert!(std::sync::Arc::ptr_eq(
        &bucket,
        &l.rate_limiter().await.unwrap()
    ));
    assert_eq!(l.qps_status().await, (5.0, 5));
    assert!(!bucket.allow().await); // Updates do not refill an exhausted bucket.
    assert_eq!(l.update_concurrency_config(2).await, UpdateStatus::Updated);
    assert_eq!(l.update_qps_config(0.0, 0).await, UpdateStatus::Updated);
    assert_eq!(l.qps_status().await, (0.0, 0));
    assert_eq!(l.update_concurrency_config(0).await, UpdateStatus::Deleted);
    // Go creates even invalid settings when no QPS limiter exists.
    assert_eq!(l.update_qps_config(-1.0, -1).await, UpdateStatus::Updated);
    assert_eq!(l.qps_status().await, (-1.0, -1));
    assert_eq!(l.update_qps_config(-1.0, -1).await, UpdateStatus::NoChange);
    assert_eq!(l.update_qps_config(1.0, 0).await, UpdateStatus::Deleted);
    assert_eq!(l.update_qps_config(2.0, 2).await, UpdateStatus::Updated);
    assert_eq!(l.update_concurrency_config(3).await, UpdateStatus::Updated);
    assert_eq!(l.update_concurrency_config(0).await, UpdateStatus::Updated);
    assert_eq!(l.update_qps_config(1e-8, 2).await, UpdateStatus::Deleted);
}

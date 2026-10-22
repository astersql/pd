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

#[tokio::test(start_paused = true)]
async fn combined_limiter_allow() {
    use astersql_pd_errs::catalog::ERR_RATE_LIMIT_EXCEEDED;
    let l = Limiter::new();
    l.update_concurrency_config(1).await;
    l.update_qps_config(1.0, 1).await;
    let first = l.allow().await.unwrap();
    assert_eq!(
        l.allow().await.err().unwrap().code(),
        ERR_RATE_LIMIT_EXCEEDED.code()
    );
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    assert!(l.allow().await.is_err()); // Concurrency denial must not consume QPS.
    first.done();
    let second = l.allow().await.unwrap();
    first.done(); // Must not release the second admission.
    assert_eq!(l.concurrency_status().await, (1, 1));
    second.done();
    assert_eq!(
        l.allow().await.err().unwrap().code(),
        ERR_RATE_LIMIT_EXCEEDED.code()
    );
    assert_eq!(l.concurrency_status().await, (1, 0)); // QPS rollback.
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    let old = l.concurrency_limiter().await;
    let done = l.allow().await.unwrap();
    l.update_concurrency_config(0).await;
    done.done();
    done.clone().done();
    assert_eq!(old.get_running_tasks_num(), 0);
    assert_eq!(l.concurrency_status().await, (0, 0));
}

#[tokio::test(start_paused = true)]
async fn combined_limiter_allow_parallel() {
    use std::sync::Arc;
    let l = Arc::new(Limiter::new());
    // Unconfigured and QPS-only admission still return completion handles.
    l.allow().await.unwrap().done();
    l.update_qps_config(1.0, 1).await;
    l.allow().await.unwrap().done();
    assert!(l.allow().await.is_err());
    assert_eq!(l.concurrency_status().await, (0, 0));
    l.update_qps_config(0.0, 0).await;
    l.update_concurrency_config(10).await;
    let mut jobs = Vec::new();
    for _ in 0..20 {
        let l = l.clone();
        jobs.push(tokio::spawn(async move { l.allow().await }));
    }
    let mut admitted = Vec::new();
    for job in jobs {
        if let Ok(done) = job.await.unwrap() {
            admitted.push(done);
        }
    }
    assert_eq!(admitted.len(), 10);
    assert_eq!(l.concurrency_status().await, (10, 10));
    let first = admitted.pop().unwrap();
    let mut releases = Vec::new();
    for _ in 0..20 {
        let first = first.clone();
        releases.push(std::thread::spawn(move || first.done()));
    }
    for release in releases {
        release.join().unwrap();
    }
    assert_eq!(l.concurrency_status().await, (10, 9));
    for done in admitted {
        done.done();
    }
    assert_eq!(l.concurrency_status().await, (10, 0));
}

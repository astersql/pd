// Copyright 2026 AsterSQL.
use crate::{StoreInfo, StoreLimiter};
use astersql_pd_core_constant::PriorityLevel;
use astersql_pd_storelimit::{SlidingWindows, Type};

#[tokio::test(start_paused = true)]
async fn core_task_19_assembly_reset_and_clone() {
    let store = StoreInfo::new(None);
    assert_eq!(store.limit_version(), "v1");
    store.reset_store_limit(Type::ADD_PEER, &[1.0, 9.0]);
    let copy = store.clone();
    assert!(
        copy.limiter()
            .unwrap()
            .write()
            .unwrap()
            .take(1000, Type::ADD_PEER, PriorityLevel::from_i32(0))
            .await
    );
    for level in 0..4 {
        assert!(
            !store
                .is_available(Type::ADD_PEER, PriorityLevel::from_i32(level))
                .await
        );
    }
    store.feedback(100.0);
    assert!(
        !store
            .is_available(Type::ADD_PEER, PriorityLevel::from_i32(0))
            .await
    );
    store.reset_store_limit(Type::ADD_PEER, &[]);
    assert!(
        copy.is_available(Type::ADD_PEER, PriorityLevel::from_i32(0))
            .await
    );
    assert!(
        store
            .is_available(Type::SEND_SNAPSHOT, PriorityLevel::from_i32(0))
            .await
    );
}

#[tokio::test(start_paused = true)]
async fn core_task_19_v2_priority_feedback_and_replacement() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_19.json")).unwrap();
    let mut store = StoreInfo::new(None);
    let old = store.clone();
    store.set_store_limit(Some(StoreLimiter::V2(SlidingWindows::new())));
    assert_eq!(old.limit_version(), "v1");
    assert_eq!(store.limit_version(), "v2");
    store.feedback(100.0); // empty window ignores feedback
    let copy = store.shallow_clone();
    for level in 0..4 {
        assert!(
            store
                .is_available(Type::SEND_SNAPSHOT, PriorityLevel::from_i32(level))
                .await
        );
        assert!(
            copy.limiter()
                .unwrap()
                .write()
                .unwrap()
                .take(1000, Type::SEND_SNAPSHOT, PriorityLevel::from_i32(level))
                .await
        );
        assert!(
            !store
                .is_available(Type::SEND_SNAPSHOT, PriorityLevel::from_i32(level))
                .await
        );
    }
    store.reset_store_limit(Type::SEND_SNAPSHOT, &[10000.0]); // v2 ignores reset
    assert!(
        !store
            .is_available(Type::SEND_SNAPSHOT, PriorityLevel::from_i32(3))
            .await
    );
    assert!(
        store
            .is_available(Type::ADD_PEER, PriorityLevel::from_i32(0))
            .await
    );
    store.feedback(fixture["feedback_error"].as_f64().unwrap());
    {
        let guard = store.limiter().unwrap().read().unwrap();
        let StoreLimiter::V2(limit) = &*guard else {
            panic!("expected v2")
        };
        assert_eq!(
            limit.capacity(),
            fixture["expected_low_capacity"].as_i64().unwrap()
        );
        assert_eq!(limit.used(), [1000; 4]);
    }

    assert!(
        copy.is_available(Type::SEND_SNAPSHOT, PriorityLevel::from_i32(0))
            .await
    );
    copy.limiter()
        .unwrap()
        .write()
        .unwrap()
        .ack(4000, Type::SEND_SNAPSHOT);
    assert!(
        store
            .is_available(Type::SEND_SNAPSHOT, PriorityLevel::from_i32(3))
            .await
    );
    store.set_store_limit(None);
    store.feedback(1.0); // nil feedback is allowed
    assert!(store.limiter().is_none());
}

#[tokio::test]
#[should_panic]
async fn core_task_19_unknown_type_panics() {
    StoreInfo::new(None)
        .is_available(Type::from_i64(99), PriorityLevel::from_i32(0))
        .await;
}

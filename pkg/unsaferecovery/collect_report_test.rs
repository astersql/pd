// Copyright 2026 AsterSQL.
use super::*;
use std::{collections::BTreeMap, sync::Arc};
fn controller() -> Controller {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(1700)));
    c.stage = Stage::CollectReport;
    c.step = 1701;
    c.recovery_start_step = 1700;
    c.expected_reports = BTreeMap::from([(1, None), (2, None)]);
    c
}
fn report(step: u64, version: u64) -> StoreReport {
    StoreReport {
        step,
        peers: vec![PeerReport {
            region: Some(astersql_pd_core::RegionMeta {
                id: 7,
                epoch: Some(astersql_pd_core::RegionEpoch {
                    version,
                    conf_ver: 3,
                }),
                ..Default::default()
            }),
            raft: Some(RaftState {
                term: 9,
                vote: 1,
                commit: 70,
                last_index: 80,
            }),
            applied_index: 75,
            is_force_leader: true,
            has_commit_merge: true,
        }],
    }
}
#[test]
fn mcs_017_behavior() {
    let mut c = controller();
    assert!(!c.collect_report(1, Some(report(1701, 1))).unwrap());
    assert_eq!(
        c.reported_store_count(),
        1,
        "a current-step report must be accepted once"
    );
    assert_eq!(c.report(1), Some(&report(1701, 1)));
    assert_eq!(c.stage(), Stage::CollectReport);
    assert!(!c.collect_report(1, Some(report(1701, 2))).unwrap());
    assert_eq!(c.reported_store_count(), 1);
    assert_eq!(c.report(1), Some(&report(1701, 2)));
    assert!(c.collect_report(2, Some(report(1701, 3))).unwrap());
    assert_eq!(c.reported_store_count(), 2);
    assert!(!c.collect_report(2, Some(report(1701, 4))).unwrap());
    assert_eq!(c.report(2), Some(&report(1701, 4)));
}
#[test]
fn mcs_017_missing_unknown_stale_and_inactive_reports_do_not_count() {
    let mut c = controller();
    for (store, step) in [(1, 1700), (1, 1702), (9, 1701)] {
        assert!(!c.collect_report(store, Some(report(step, 1))).unwrap());
    }
    assert!(!c.collect_report(1, None).unwrap());
    assert_eq!(c.reported_store_count(), 0);
    c.failed_stores.insert(9);
    assert_eq!(
        c.collect_report(9, None).unwrap_err().to_string(),
        "Receive heartbeat from Failed store 9"
    );
    c.collect_report(1, Some(report(1701, 1))).unwrap();
    let accepted = c.report(1).cloned();
    c.collect_report(1, Some(report(1700, 99))).unwrap();
    assert_eq!(c.report(1), accepted.as_ref());
    for stage in [Stage::Idle, Stage::Finished, Stage::Failed] {
        c.stage = stage;
        assert!(!c.collect_report(2, Some(report(1701, 1))).unwrap());
        assert_eq!(c.reported_store_count(), 1);
    }
}
#[test]
fn mcs_017_dispatch_handshake_uses_exact_step_and_strict_retry_deadline() {
    let mut c = controller();
    c.plan_execution_timeout_nanos = 500;
    assert_eq!(c.request_report(1, 1000), Some(1701));
    assert_eq!(c.request_report(1, 1499), None);
    assert_eq!(c.request_report(1, 1500), None);
    assert_eq!(c.request_report(1, 1501), Some(1701));
    assert_eq!(c.request_report(9, 9999), None);
    c.collect_report(1, Some(report(1701, 1))).unwrap();
    assert_eq!(c.request_report(1, 9999), None);
    // A new stage has its own step; an earlier stage's report cannot fill its slot.
    c.step = 1702;
    c.stage = Stage::ForceLeader;
    c.expected_reports.insert(1, None);
    c.num_stores_reported = 0;
    assert!(!c.collect_report(1, Some(report(1701, 1))).unwrap());
    assert_eq!(c.reported_store_count(), 0);
    assert!(!c.collect_report(1, Some(report(1702, 1))).unwrap());
    assert_eq!(c.reported_store_count(), 1);
}

// Copyright 2026 AsterSQL.
use super::*;
use std::sync::Arc;
#[test]
fn mcs_023_behavior() {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(100)));
    c.stage = Stage::ForceLeader;
    c.step = 102;
    c.expected_reports.insert(1, None);
    assert_eq!(
        c.request_report(1, 10),
        Some(102),
        "every running phase must dispatch its exact current step"
    );
    assert_eq!(c.request_report(1, 60_000_000_010), None);
    assert_eq!(c.request_report(1, 60_000_000_011), Some(102));
}

fn report_round(c: &mut Controller, region: &astersql_pd_core::RegionMeta, force: bool) {
    let step = c.step();
    for store in [1, 2] {
        assert_eq!(
            c.collect_report(
                store,
                Some(StoreReport {
                    step,
                    peers: vec![PeerReport {
                        region: Some(region.clone()),
                        is_force_leader: force && store == 2,
                        ..Default::default()
                    }]
                })
            )
            .unwrap(),
            store == 2
        );
    }
}
#[tokio::test]
async fn mcs_023_force_demote_exit_confirmations_and_finished_cache_signal() {
    let fixture = etcd_fixture::Fixture::start();
    let (term, mut ids) =
        crate::empty_region_test::ids(Arc::new(fixture.store().await), 1000).await;
    let (mut c, index, stores) = crate::force_leader_test::fixture();
    c.stage = Stage::CollectReport;
    c.recovery_start_step = 18;
    c.step = 19;
    c.num_stores_reported = 2;
    let mut region = index.regions.items().next().unwrap().region().clone();
    let first = c
        .advance_reports(&index, &stores, &mut ids, 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.stage, Stage::ForceLeader);
    let step = c.step();
    assert!(c.reports().values().all(Option::is_none));
    let plan = c.dispatch_plan(2, 10).unwrap();
    assert_eq!(plan.step, step);
    assert_eq!(plan.force_leader.unwrap().enter_force_leaders, [7]);
    assert!(c.dispatch_plan(2, 60_000_000_010).is_none());
    assert_eq!(c.dispatch_plan(2, 60_000_000_011).unwrap().step, step);
    assert!(c.dispatch_plan(77, 0).is_none());
    assert!(
        !c.collect_report(
            1,
            Some(StoreReport {
                step: step.wrapping_sub(1),
                ..Default::default()
            })
        )
        .unwrap()
    );
    assert_eq!(c.reported_store_count(), 0);
    report_round(&mut c, &region, true);
    assert!(c.dispatch_plan(2, i128::MAX).is_none());
    let index = c.build_region_index().unwrap();
    assert_eq!(
        c.advance_reports(&index, &stores, &mut ids, 2)
            .await
            .unwrap()
            .unwrap()
            .stage,
        Stage::DemoteFailedVoter
    );
    let demote_step = c.step();
    assert_ne!(demote_step, step);
    assert_eq!(
        c.dispatch_plan(2, 3).unwrap().demotes[0]
            .failed_voters
            .len(),
        3
    );
    region.peers.retain(|p| p.store_id <= 2);
    report_round(&mut c, &region, true);
    let index = c.build_region_index().unwrap();
    assert_eq!(
        c.advance_reports(&index, &stores, &mut ids, 4)
            .await
            .unwrap()
            .unwrap()
            .stage,
        Stage::ExitForceLeader
    );
    let exit = c.dispatch_plan(2, 5).unwrap();
    assert!(exit.force_leader.is_none());
    assert!(exit.creates.is_empty());
    assert!(exit.demotes.is_empty());
    assert!(exit.tombstones.is_empty());
    assert!(c.stage().is_running());
    report_round(&mut c, &region, false);
    let index = c.build_region_index().unwrap();
    assert!(
        c.advance_reports(&index, &stores, &mut ids, 6)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(c.stage(), Stage::Finished);
    assert!(c.take_reset_region_cache());
    assert!(!c.take_reset_region_cache());
    assert!(c.dispatch_plan(1, 7).is_none());
    assert_eq!(c.show(7), c.show(800));
    let history = c.show(7);
    assert_eq!(history.len(), 4);
    assert_eq!(history[0].actions["store 1"], ["force leader on regions: "]);
    assert_eq!(
        history.last().unwrap().details,
        ["no newly created empty regions"]
    );
    term.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_023_collect_only_finish_keeps_cache_and_demote_rechecks_force() {
    let fixture = etcd_fixture::Fixture::start();
    let (term, mut ids) =
        crate::empty_region_test::ids(Arc::new(fixture.store().await), 1000).await;
    let (mut c, index, stores) = crate::force_leader_test::fixture();
    c.stage = Stage::DemoteFailedVoter;
    c.num_stores_reported = 2;
    let prepared = c
        .advance_reports(&index, &stores, &mut ids, 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        prepared.stage,
        Stage::ForceLeader,
        "demote no-work must recheck prior force phases once"
    );
    let mut region = index.regions.items().next().unwrap().region().clone();
    region.peers.retain(|p| p.store_id <= 2);
    c.stage = Stage::CollectReport;
    c.recovery_start_step = c.step.wrapping_sub(1);
    c.plans.clear();
    report_round(&mut c, &region, false);
    let index = c.build_region_index().unwrap();
    assert!(
        c.advance_reports(&index, &stores, &mut ids, 1)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(c.stage(), Stage::Finished);
    assert!(!c.take_reset_region_cache());
    term.resign().await.unwrap();
}
#[test]
fn mcs_023_show_classifies_dispatch_and_reports_without_mutating_history() {
    let mut c = Controller::new(Arc::new(RecoverySteps::new(0)));
    assert_eq!(c.show(0)[0].info, "No on-going recovery.");
    assert!(c.show(0)[0].unix_nanos.is_none());
    c.auto_detect = true;
    c.expected_reports = [(1, None), (2, None), (3, None)].into();
    c.change_stage(Stage::CollectReport, 1);
    c.dispatch_plan(2, 2).unwrap();
    c.collect_report(
        3,
        Some(StoreReport {
            step: c.step(),
            ..Default::default()
        }),
    )
    .unwrap();
    let show = c.show(3);
    assert_eq!(
        show[0].details,
        ["auto detect mode with no specified Failed stores"]
    );
    assert_eq!(show[1].info, "Collecting reports from alive stores(1/3)");
    assert_eq!(
        show[1].details,
        [
            "Stores that have not dispatched plan: 1",
            "Stores that have reported to PD: 3",
            "Stores that have not reported to PD: 2"
        ]
    );
    assert_eq!(c.output.len(), 1);
}

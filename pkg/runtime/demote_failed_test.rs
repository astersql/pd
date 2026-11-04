// Copyright 2026 AsterSQL.
use crate::{
    NodeConfig, PdNodeBuilder,
    collect_report_test::heartbeat,
    node_test::fixture,
    recovery_index_test::{ReportOptions, report_all_options},
    recovery_start_test::http,
};
use astersql_pd_proto::metapb;
use astersql_pd_unsaferecovery::{RecoveryPlan, Stage};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_020_native_demote_payloads_preserve_go_joint_learner_and_overlap_rules() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    for options in [
        ReportOptions {
            forced: true,
            ..Default::default()
        },
        ReportOptions {
            forced: true,
            joint: true,
            ..Default::default()
        },
        ReportOptions {
            forced: true,
            failed_learner: true,
            ..Default::default()
        },
        ReportOptions {
            forced: true,
            healthy_overlap: true,
            ..Default::default()
        },
    ] {
        let binary = pinned_go_binary().unwrap();
        let mut go = Server::start(&binary).unwrap();
        go.ready_with_timeout(Duration::from_secs(12)).unwrap();
        let (mut client, cluster, step, completed) = report_all_options(&go.url, options).await;
        let mut plans = BTreeMap::from([(
            3,
            completed
                .recovery_plan
                .expect("actual Go demote completion must request its next phase"),
        )]);
        for store in [1, 2] {
            plans.insert(
                store,
                heartbeat(&mut client, cluster, store, 1, None)
                    .await
                    .recovery_plan
                    .unwrap(),
            );
        }
        let show = http(&go.url, "admin/unsafe/remove-failed-stores/show", None);
        assert_eq!(
            show.1[1]["info"],
            "Unsafe recovery enters demote Failed voter stage"
        );
        let actual_step = plans[&3].step;
        assert_eq!(actual_step, step.wrapping_add(1));
        assert!(plans.values().all(|p| p.step == actual_step));
        let fixture = fixture::Fixture::start();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    Arc::new(fixture.store().await),
                    NodeConfig {
                        cluster_id: 2000,
                        member_id: 20,
                        name: "demote".into(),
                        client_url: url.clone(),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        let (_, _, rust_step, _) = report_all_options(&url, options).await;
        let prepared = node
            .cluster()
            .unsafe_recovery_plan()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(prepared.stage, Stage::DemoteFailedVoter);
        for store in [1, 2, 3] {
            let domain = prepared
                .batch
                .plans
                .get(&store)
                .cloned()
                .unwrap_or_else(RecoveryPlan::default);
            assert_eq!(domain.step, 0);
            let wire = astersql_pd_server::recovery_report_adapter::plan_to_proto(domain);
            let go = &plans[&store];
            assert_eq!(wire.demotes, go.demotes, "{options:?}, store={store}");
            assert_eq!(wire.tombstones, go.tombstones);
            assert_eq!(wire.force_leader, go.force_leader);
            assert_eq!(wire.creates, go.creates);
        }
        let affected = plans
            .values()
            .flat_map(|p| {
                p.tombstones
                    .iter()
                    .copied()
                    .chain(p.demotes.iter().map(|d| d.region_id))
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(prepared.batch.affected_regions, affected);
        let failed = &prepared.batch.plans[&2].demotes[0].failed_voters;
        assert!(failed.iter().all(|p| ![1, 2].contains(&p.store_id)));
        if options.joint {
            assert!(failed.is_empty())
        }
        if options.failed_learner {
            assert!(
                plans[&2].demotes[0]
                    .failed_voters
                    .iter()
                    .any(|p| p.store_id == 6 && p.role == metapb::PeerRole::Learner as i32)
            );
        }
        assert_eq!(
            prepared.batch.plans[&1].tombstones.is_empty(),
            options.healthy_overlap
        );
        node.shutdown().await.unwrap();
        go.stop().unwrap();
        println!(
            "{options:?}: Go collect/demote steps {step}/{actual_step}, Rust collect {rust_step}; exact 3-store payloads and affected regions {affected:?}; no healthy surviving peer listed"
        );
    }
}

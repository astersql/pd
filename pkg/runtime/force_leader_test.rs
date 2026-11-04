// Copyright 2026 AsterSQL.
use crate::{
    NodeConfig, PdNodeBuilder, collect_report_test::heartbeat, node_test::fixture,
    recovery_index_test::report_all_for_mode, recovery_start_test::http,
};
use astersql_pd_unsaferecovery::Stage;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_019_native_prepared_force_and_commit_merge_plans_match_go() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    for (commit, joint) in [(false, false), (true, false), (false, true)] {
        let binary = pinned_go_binary().unwrap();
        let mut go = Server::start(&binary).unwrap();
        go.ready_with_timeout(Duration::from_secs(12)).unwrap();
        let (mut client, cluster, step, completed) =
            report_all_for_mode(&go.url, commit, joint).await;
        let mut plans = BTreeMap::from([(
            3,
            completed
                .recovery_plan
                .expect("Go's completion must dispatch the new phase"),
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
        let stage = if commit {
            Stage::ForceLeaderForCommitMerge
        } else {
            Stage::ForceLeader
        };
        let show = http(&go.url, "admin/unsafe/remove-failed-stores/show", None);
        assert_eq!(
            show.1[1]["info"],
            if commit {
                "Unsafe recovery enters force leader for commit merge stage"
            } else {
                "Unsafe recovery enters force leader stage"
            }
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
                        cluster_id: 1900,
                        member_id: 19,
                        name: "force".into(),
                        client_url: url.clone(),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        let (_, _, rust_step, _) = report_all_for_mode(&url, commit, joint).await;
        let prepared = node
            .cluster()
            .unsafe_recovery_plan()
            .await
            .unwrap()
            .expect("real node must prepare a plan from received reports");
        assert_eq!(prepared.stage, stage);
        assert_eq!(prepared.batch.plans.len(), 3);
        for (store, domain) in &prepared.batch.plans {
            assert_eq!(
                domain.step, 0,
                "phase step is assigned by later orchestration, not the generator"
            );
            let wire = astersql_pd_server::recovery_report_adapter::plan_to_proto(domain.clone());
            let go = &plans[store];
            assert_eq!(
                wire.force_leader, go.force_leader,
                "commit={commit},joint={joint},store={store}"
            );
            assert_eq!(wire.creates, go.creates);
            assert_eq!(wire.tombstones, go.tombstones);
            assert_eq!(wire.demotes, go.demotes);
        }
        let affected = plans
            .values()
            .flat_map(|p| {
                p.force_leader
                    .as_ref()
                    .unwrap()
                    .enter_force_leaders
                    .iter()
                    .copied()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(prepared.batch.affected_regions, affected);
        if joint {
            assert!(
                prepared.batch.plans[&2]
                    .force_leader
                    .as_ref()
                    .unwrap()
                    .failed_stores
                    .is_empty(),
                "incoming quorum survives joint exit"
            );
        }
        node.shutdown().await.unwrap();
        go.stop().unwrap();
        println!(
            "commit={commit},joint={joint}: Go collect/force steps {step}/{actual_step}, Rust collect step {rust_step}; all three prepared plan payloads and affected regions {affected:?} equal Go; generator steps remain unassigned"
        );
    }
}

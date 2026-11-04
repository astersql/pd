// Copyright 2026 AsterSQL.
use crate::{
    NodeConfig, PdNodeBuilder,
    collect_report_test::heartbeat,
    node_test::fixture,
    recovery_dispatch_test::normalized_frame,
    recovery_start_test::{http, seed},
};
use astersql_pd_proto::pdpb;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Clone, Copy, Debug)]
enum Mode {
    Abort,
    HeartbeatTimeout,
    NoHeartbeatTimeout,
}
fn show(url: &str, original: &mut Option<String>) -> Value {
    let (status, mut show) = http(url, "admin/unsafe/remove-failed-stores/show", None);
    assert_eq!(status, 200);
    println!("actual Show {show}");
    for event in show.as_array_mut().unwrap() {
        if let Some(time) = event.get_mut("time") {
            assert_eq!(time.as_str().unwrap().len(), 23);
            *time = json!("wall-clock");
        }
        if let Some(details) = event.get_mut("details").and_then(Value::as_array_mut) {
            for detail in details {
                let text = detail.as_str().unwrap();
                if let Some((prefix, deadline)) = text.split_once("Exceeds timeout ") {
                    validate_deadline(deadline);
                    if let Some(original) = original.as_ref() {
                        assert_eq!(
                            deadline, original,
                            "the earliest absolute deadline error survives exit failure"
                        );
                    } else {
                        *original = Some(deadline.into());
                    }
                    *detail = json!(format!("{prefix}Exceeds timeout wall-clock-deadline"));
                } else if text.starts_with("Stores that have ") {
                    let (prefix, ids) = text.split_once(": ").unwrap();
                    let mut ids = ids.split(", ").collect::<Vec<_>>();
                    ids.sort();
                    *detail = json!(format!("{prefix}: {}", ids.join(", ")));
                }
            }
        }
        if let Some(info) = event.get_mut("info") {
            if let Some((prefix, deadline)) = info.as_str().unwrap().split_once("Exceeds timeout ")
            {
                validate_deadline(deadline);
                assert_eq!(Some(deadline), original.as_deref());
                *info = json!(format!("{prefix}Exceeds timeout wall-clock-deadline"));
            }
        }
    }
    show
}
fn validate_deadline(text: &str) {
    let (wall, monotonic) = text
        .split_once(" m=")
        .expect("actual Go-compatible monotonic projection");
    let parts = wall.split(' ').collect::<Vec<_>>();
    assert_eq!(parts.len(), 4);
    chrono::NaiveDateTime::parse_from_str(
        &format!("{} {}", parts[0], parts[1]),
        "%Y-%m-%d %H:%M:%S%.f",
    )
    .unwrap();
    assert_eq!(parts[2].len(), 5);
    assert!(parts[2].starts_with('+') || parts[2].starts_with('-'));
    assert!(!parts[3].is_empty());
    assert!(monotonic.parse::<f64>().unwrap().is_finite());
}
async fn observe(url: &str, mode: Mode) -> (Vec<Value>, Vec<pdpb::StoreHeartbeatResponse>) {
    let (mut client, cluster, _) = seed(url).await;
    let abort_path = "admin/unsafe/remove-failed-stores/abort";
    let invalid = http(url, abort_path, Some(&json!({})));
    assert_eq!(invalid.0, 400);
    let mut output = vec![json!(invalid)];
    assert_eq!(
        http(
            url,
            "admin/unsafe/remove-failed-stores",
            Some(&json!({"auto-detect":true,"timeout":1,"plan-execution-timeout":1}))
        )
        .0,
        200
    );
    let initial = heartbeat(&mut client, cluster, 1, 101, None).await;
    let collect = initial.recovery_plan.as_ref().unwrap().step;
    let mut frames = vec![normalized_frame(initial, cluster, collect)];
    let mut original = None;
    match mode {
        Mode::Abort => {
            let result = http(url, abort_path, Some(&json!({})));
            assert_eq!(result.0, 200);
            output.push(json!(result));
        }
        Mode::HeartbeatTimeout | Mode::NoHeartbeatTimeout => {
            tokio::time::sleep(Duration::from_millis(1100)).await;
        }
    }
    if matches!(mode, Mode::HeartbeatTimeout) {
        let response = heartbeat(&mut client, cluster, 1, 101, None).await;
        assert_eq!(response.recovery_plan.as_ref().unwrap().step, collect + 1);
        frames.push(normalized_frame(response, cluster, collect));
    }
    let exiting = show(url, &mut original);
    assert_eq!(
        exiting[1]["info"],
        "Unsafe recovery enters exit force leader stage"
    );
    output.push(exiting);
    let repeated = http(url, abort_path, Some(&json!({})));
    assert_eq!(repeated.0, 200);
    output.push(json!(repeated));
    if matches!(mode, Mode::NoHeartbeatTimeout) {
        // No store heartbeat after start: Show performs both timeout checks, as in Go.
        tokio::time::sleep(Duration::from_millis(2200)).await;
    } else {
        for store in 1..=3 {
            let response = heartbeat(
                &mut client,
                cluster,
                store,
                999,
                Some(pdpb::StoreReport {
                    step: collect + 1,
                    peer_reports: vec![],
                }),
            )
            .await;
            if store < 3 {
                assert!(response.recovery_plan.is_none());
            }
            frames.push(normalized_frame(response, cluster, collect));
        }
    }
    let failed = show(url, &mut original);
    let info = failed.as_array().unwrap().last().unwrap()["info"]
        .as_str()
        .unwrap();
    assert_eq!(
        info,
        if matches!(mode, Mode::Abort) {
            "Unsafe recovery Failed: aborted by operator"
        } else {
            "Unsafe recovery Failed: Exceeds timeout wall-clock-deadline"
        }
    );
    output.push(failed);
    for store in 1..=3 {
        let response = heartbeat(&mut client, cluster, store, 101, None).await;
        assert!(response.recovery_plan.is_none());
        frames.push(normalized_frame(response, cluster, collect));
    }
    let invalid = http(url, abort_path, Some(&json!({})));
    assert_eq!(invalid.0, 400);
    output.push(json!(invalid));
    assert_eq!(
        http(
            url,
            "admin/unsafe/remove-failed-stores",
            Some(&json!({"auto-detect":true}))
        )
        .0,
        200
    );
    let stale = heartbeat(
        &mut client,
        cluster,
        1,
        999,
        Some(pdpb::StoreReport {
            step: collect,
            peer_reports: vec![],
        }),
    )
    .await;
    assert_eq!(
        stale.recovery_plan.as_ref().unwrap().step,
        collect + 4,
        "new recovery cannot reuse completed or aborted phase steps"
    );
    frames.push(normalized_frame(stale, cluster, collect));
    let restart = show(url, &mut None);
    assert_eq!(
        restart.as_array().unwrap().last().unwrap()["info"],
        "Collecting reports from alive stores(0/3)"
    );
    output.push(restart);
    println!(
        "mode={mode:?},cluster={cluster},actual collect={collect},frames={frames:?},normalized Show={output:?}"
    );
    (output, frames)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_024_native_abort_and_heartbeat_free_timeouts_match_go() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    for mode in [
        Mode::Abort,
        Mode::HeartbeatTimeout,
        Mode::NoHeartbeatTimeout,
    ] {
        let mut go = Server::start(&pinned_go_binary().unwrap()).unwrap();
        go.ready_with_timeout(Duration::from_secs(12)).unwrap();
        let expected = observe(&go.url, mode).await;
        let fixture = fixture::Fixture::start();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    Arc::new(fixture.store().await),
                    NodeConfig {
                        cluster_id: 2400,
                        member_id: 24,
                        name: "abort".into(),
                        client_url: url.clone(),
                        peer_url: "http://127.0.0.1:2380".into(),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        let observed = observe(&url, mode).await;
        assert_eq!(
            observed, expected,
            "mode={mode:?}: exact HTTP results and complete heartbeat frames; actual clock values separately retained and verified"
        );
        node.cluster().abort_unsafe_recovery().await.unwrap();
        node.shutdown().await.unwrap();
        go.stop().unwrap();
    }
}

// Copyright 2026 AsterSQL.
use crate::{Identity, PdServices, service_audit::*};
use astersql_pd_proto::pdpb;
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use prost::Message;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Capture {
    records: Mutex<Vec<Record>>,
    fail: std::sync::atomic::AtomicBool,
}
impl Sink for Capture {
    fn record(&self, record: Record) -> Result<(), String> {
        self.records.lock().unwrap().push(record);
        if self.fail.load(Ordering::SeqCst) {
            Err("sink unavailable".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Default)]
struct Tick(AtomicU64);
impl Clock for Tick {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.fetch_add(7, Ordering::SeqCst))
    }
}
struct Services {
    identity: Identity,
    store: Arc<dyn MetaStore>,
    entered: tokio::sync::Notify,
}
#[async_trait::async_trait]
impl PdServices for Services {
    fn identity(&self) -> &Identity {
        &self.identity
    }
    fn recovery_marker_store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.store.clone())
    }
    fn discovery_store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.store.clone())
    }
    async fn healthy(&self) -> Result<(), String> {
        Ok(())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Ok(81)
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("unused".into())
    }
    async fn scheduler_config(
        &self,
        _: axum::http::Request<axum::body::Body>,
    ) -> Result<axum::response::Response, String> {
        self.entered.notify_one();
        let mut watch = self
            .store
            .watch(b"/task081/wait".to_vec(), 1)
            .await
            .map_err(|e| e.to_string())?;
        watch
            .next(&crate::request_context::current().unwrap().cancel)
            .await
            .map_err(|e| e.to_string())?;
        Err("cancelled".into())
    }
}
fn call(url: &str, method: &str, path: &str, body: &str) -> (u16, String) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "-H",
            "Authorization: Bearer secret",
            "-H",
            "X-Caller-ID: private",
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v1/{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), body.into())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_081_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let member = pdpb::Member {
        member_id: 1,
        name: "audit".into(),
        client_urls: vec![url.clone()],
        ..Default::default()
    };
    store
        .transact(
            vec![],
            vec![
                Mutation::put(b"/pd/81/leader", &member.encode_to_vec()),
                Mutation::put(b"/pd/81/rust/members/1", &member.encode_to_vec()),
            ],
        )
        .await
        .unwrap();
    let services = Arc::new(Services {
        identity: Identity {
            cluster_id: 81,
            member_id: 1,
            name: "audit".into(),
            client_url: url.clone(),
            peer_url: "http://peer".into(),
        },
        store: store.clone(),
        entered: tokio::sync::Notify::new(),
    });
    let capture = Arc::new(Capture::default());
    let cancel = CancellationToken::new();
    let app = crate::router_with_audit(
        services.clone(),
        cancel.clone(),
        Some(Audit::with_clock(
            capture.clone(),
            Arc::new(Tick::default()),
        )),
    );
    let stop = cancel.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    assert_eq!(call(&url, "GET", "ping?token=secret", "").0, 200);
    assert_eq!(
        capture.records.lock().unwrap().as_slice(),
        &[Record {
            service: "Ping",
            policy: Policy {
                local_log: false,
                prometheus: true
            },
            status: Some(200),
            elapsed: Duration::from_millis(7)
        }]
    );
    // Parsing errors are audited without retaining malformed bodies or queries.
    assert_eq!(
        call(
            &url,
            "POST",
            "service-middleware/config?password=secret",
            "secret"
        )
        .0,
        400
    );
    assert_eq!(
        capture.records.lock().unwrap().last().unwrap().status,
        Some(400)
    );
    assert!(
        capture
            .records
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .policy
            .local_log
    );
    // A failing sink does not change the durable configuration mutation or reply.
    capture.fail.store(true, Ordering::SeqCst);
    assert_eq!(
        call(
            &url,
            "POST",
            "service-middleware/config",
            r#"{"audit.enable-audit":"false"}"#
        )
        .0,
        200
    );
    let count = capture.records.lock().unwrap().len();
    assert_eq!(call(&url, "GET", "ping", "").0, 200);
    assert_eq!(capture.records.lock().unwrap().len(), count);
    let saved = store
        .read(Read::prefix(
            b"/pd/81/service_middleware",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert!(!saved.entries.is_empty());
    assert!(
        String::from_utf8(saved.entries[0].value.clone())
            .unwrap()
            .contains("false")
    );
    // A newly assembled router must load the durable disabled audit switch.
    let recovered_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let recovered_url = format!("http://{}", recovered_listener.local_addr().unwrap());
    let recovered_cancel = CancellationToken::new();
    let recovered_app = crate::router_with_audit(
        services.clone(),
        recovered_cancel.clone(),
        Some(Audit::new(capture.clone())),
    );
    let recovered_stop = recovered_cancel.clone();
    let recovered_server = tokio::spawn(async move {
        axum::serve(recovered_listener, recovered_app)
            .with_graceful_shutdown(recovered_stop.cancelled_owned())
            .await
            .unwrap();
    });
    assert_eq!(call(&recovered_url, "GET", "ping", "").0, 200);
    assert_eq!(capture.records.lock().unwrap().len(), count);
    recovered_cancel.cancel();
    recovered_server.await.unwrap();
    capture.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        call(
            &url,
            "POST",
            "service-middleware/config",
            r#"{"audit.enable-audit":"true"}"#
        )
        .0,
        200
    );
    assert_eq!(call(&url, "GET", "ping", "").0, 200);
    assert_eq!(capture.records.lock().unwrap().len(), count + 1);
    let before = capture.records.lock().unwrap().len();
    let _ = call(&url, "GET", "unknown-secret", "");
    let mut client = pdpb::pd_client::PdClient::connect(url.clone())
        .await
        .unwrap();
    client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap();
    assert_eq!(capture.records.lock().unwrap().len(), before);
    if let Ok(path) = std::env::var("PD_AUDIT_GO_REPORT") {
        let go: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let records = capture.records.lock().unwrap();
        let statuses: Vec<_> = records.iter().map(|r| r.status.unwrap()).collect();
        assert_eq!(statuses, [200, 400, 200, 200]);
        assert_eq!(
            go["counts"].as_array().unwrap().last().unwrap(),
            &serde_json::json!({"Ping": records.iter().filter(|r| r.service == "Ping").count(),
               "SetServiceMiddlewareConfig": records.iter().filter(|r| r.service == "SetServiceMiddlewareConfig").count()})
        );
        assert_eq!(
            go["local_log_count"],
            records.iter().filter(|r| r.policy.local_log).count()
        );
        assert_eq!(go["config"][1]["audit"]["enable-audit"], "true");
    }
    // A TCP disconnect while reading an incomplete request drops the audit guard.
    use tokio::io::AsyncWriteExt;
    let mut incomplete = tokio::net::TcpStream::connect(url.strip_prefix("http://").unwrap())
        .await
        .unwrap();
    incomplete.write_all(b"POST /pd/api/v1/service-middleware/config HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\nsecret").await.unwrap();
    // A separate TCP probe ensures the listener remains responsive during the pending body.
    assert_eq!(call(&url, "GET", "ping", "").0, 200);
    drop(incomplete);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if capture.records.lock().unwrap().len() == before + 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let before = capture.records.lock().unwrap().len();
    // Cancel a real pending MetaStore watch through the listener's host scope.
    let pending_url = url.clone();
    let pending = tokio::task::spawn_blocking(move || {
        call(
            &pending_url,
            "GET",
            "scheduler-config/wait?token=secret",
            "",
        )
    });
    tokio::time::timeout(Duration::from_secs(5), services.entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    assert_eq!(pending.await.unwrap().0, 500);
    server.await.unwrap();
    let records = capture.records.lock().unwrap();
    assert_eq!(records.len(), before + 1);
    assert_eq!(records.last().unwrap().service, "GetSchedulerConfig");
    assert_eq!(records.last().unwrap().status, Some(500));
    println!("TASK081 access records: {records:?}");
    let debug = format!("{records:?}");
    for secret in [
        "secret", "private", "Bearer", "password", "token", "unknown-",
    ] {
        assert!(!debug.contains(secret));
    }
}
#[test]
fn task_081_policy() {
    assert_eq!(
        policy("Profile"),
        Some(Policy {
            local_log: true,
            prometheus: false
        })
    );
    assert_eq!(
        policy("GetMembers"),
        Some(Policy {
            local_log: false,
            prometheus: true
        })
    );
    assert_eq!(policy("GetClusterInfo"), None);
    assert_eq!(policy(""), None);
}

#[test]
fn task_081_go_route_policy_inventory() {
    // Read the checked-in Go oracle as data; no Go code is built or modified.
    let source = include_str!("../../server/api/router.go");
    let mut checked = std::collections::BTreeSet::new();
    for line in source.lines() {
        if !line.contains("registerFunc(") && !line.contains("registerPrefix(") {
            continue;
        }
        let Some((registration, backends)) = line.split_once("setAuditBackend(") else {
            continue;
        };
        let fields: Vec<_> = registration.split(',').collect();
        let Some(handler) = fields.get(2) else {
            continue;
        };
        let handler = handler.trim();
        let label = if handler.starts_with('"') {
            handler.trim_matches('"')
        } else {
            handler.rsplit('.').next().unwrap()
        };
        if label.contains('(') {
            continue;
        }
        assert_eq!(
            policy(label),
            Some(Policy {
                local_log: backends.starts_with("localLog"),
                prometheus: backends.split(')').next().unwrap().contains("prometheus")
            }),
            "{label}"
        );
        checked.insert(label);
    }
    // Three multiline recovery registrations are covered by the same named policy.
    for label in [
        "RemoveFailedStores",
        "AbortFailedStoresRemoval",
        "GetFailedStoresRemovalStatus",
    ] {
        assert!(policy(label).is_some());
    }
    assert!(
        checked.len() >= 150,
        "must inspect the real route table, got {}",
        checked.len()
    );
}

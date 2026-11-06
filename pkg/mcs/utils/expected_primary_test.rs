// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::{ClusterPaths, MsParam};
use astersql_pd_mcs_discovery::ServiceRegistryEntry;
use astersql_pd_storage_domain::*;
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../../storage/kv/etcd_fixture_test.rs"]
mod fixture;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_055_transfer_fences_old_term_and_marker_cleanup() {
    let mut f = fixture::Fixture::start();
    let s: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let paths = ClusterPaths::new(55);
    let param = MsParam {
        service_name: "tso".into(),
        group_id: 3,
    };
    let c = tokio_util::sync::CancellationToken::new();
    let key = paths.election_path(Some(&param));
    let term = astersql_pd_election::Leadership::campaign_with_key(
        s.clone(),
        c.clone(),
        55,
        key.into_bytes(),
        b"one".to_vec(),
        5,
        vec![],
    )
    .await
    .unwrap();
    let one = ServiceRegistryEntry {
        name: "one".into(),
        service_addr: "http://one".into(),
        start_timestamp: 1,
        ..Default::default()
    };
    let two = ServiceRegistryEntry {
        name: "two".into(),
        service_addr: "http://two".into(),
        start_timestamp: 2,
        ..Default::default()
    };
    let old = ServiceRegistryEntry {
        service_addr: "https://old".into(),
        start_timestamp: 0,
        ..two.clone()
    };
    assert_eq!(latest_entries(&[one.clone(), old, two.clone()]).len(), 2);
    assert!(is_valid_primary_candidate(
        &[two.clone()],
        "two",
        &["https://two".into()]
    ));
    assert!(!is_valid_primary_candidate(
        &[two.clone()],
        "two",
        &["http://other".into()]
    ));
    transfer_primary(
        s.as_ref(),
        &term,
        &paths,
        &param,
        &[one.clone(), two.clone()],
        "one",
        "two",
        &[one.service_addr, two.service_addr],
        5,
    )
    .await
    .unwrap();
    assert!(term.is_expired());
    assert!(term.validate().await.is_err());
    let entry = expected_primary(s.as_ref(), &paths, &param)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.value, b"http://two");
    assert!(entry.lease.is_some());
    assert!(
        transfer_primary(
            s.as_ref(),
            &term,
            &paths,
            &param,
            &[],
            "one",
            "missing",
            &[],
            5
        )
        .await
        .is_err()
    );
    let next = s.grant(10).await.unwrap();
    let marker = paths.expected_primary_path(&param).into_bytes();
    s.transact(
        vec![],
        vec![Mutation::Put {
            key: marker.clone(),
            value: b"three".to_vec(),
            lease: Some(next.id),
        }],
    )
    .await
    .unwrap();
    clear_expected_primary(s.as_ref(), marker.clone(), &entry)
        .await
        .unwrap();
    let observed = expected_primary(s.as_ref(), &paths, &param)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(observed.value, b"three");
    clear_expected_primary(s.as_ref(), marker, &observed)
        .await
        .unwrap();
    assert!(
        expected_primary(s.as_ref(), &paths, &param)
            .await
            .unwrap()
            .is_none()
    );
    assert!(s.ttl(next.id).await.unwrap_or(-1) <= 0);
    f.command("stop", "stopped");
    assert!(expected_primary(s.as_ref(), &paths, &param).await.is_err());
    f.command("start", "restarted");
    assert!(
        expected_primary(s.as_ref(), &paths, &param)
            .await
            .unwrap()
            .is_none()
    );
}

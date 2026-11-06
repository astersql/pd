// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::{Consistency, MetaStore, Mutation, Read};
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../../storage/kv/etcd_fixture_test.rs"]
mod fixture;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_058_registry_and_lease_ownership() {
    let f = fixture::Fixture::start();
    let s: Arc<dyn MetaStore> = Arc::new(f.store().await);
    let c = CancellationToken::new();
    let entry = ServiceRegistryEntry {
        name: "one".into(),
        service_addr: "http://one".into(),
        start_timestamp: 7,
        ..Default::default()
    };
    let escaped = ServiceRegistryEntry {
        name: "<>&\u{2028}".into(),
        ..Default::default()
    };
    assert!(
        String::from_utf8(escaped.serialize().unwrap())
            .unwrap()
            .contains("\\u003c\\u003e\\u0026\\u2028")
    );
    assert_eq!(
        ServiceRegistryEntry::deserialize(&entry.serialize().unwrap()).unwrap(),
        entry
    );
    assert_eq!(
        ServiceRegistryEntry::deserialize(br#"{"name":"first","NAME":"last","name":null}"#)
            .unwrap()
            .name,
        "last"
    );
    assert!(get_ms_members(s.as_ref(), 58, "unknown", &c).await.is_err());
    assert_eq!(
        get_ms_members(s.as_ref(), 58, "tso", &c).await.unwrap(),
        None
    );
    let reg = ServiceRegister::start(
        s.clone(),
        tokio::runtime::Handle::current(),
        c.clone(),
        58,
        "tso",
        "http://one",
        entry.serialize().unwrap(),
        2,
    )
    .await
    .unwrap();
    assert!(
        ServiceRegister::start(
            s.clone(),
            tokio::runtime::Handle::current(),
            c.clone(),
            58,
            "tso",
            "http://one",
            entry.serialize().unwrap(),
            2
        )
        .await
        .is_err()
    );
    s.transact(
        vec![],
        vec![Mutation::put(b"/ms/58/tso/registry/corrupt", b"broken")],
    )
    .await
    .unwrap();
    assert_eq!(
        get_ms_members(s.as_ref(), 58, "tso", &c).await.unwrap(),
        Some(vec![entry.clone()])
    );
    let key = b"/ms/58/tso/registry/http://one";
    let old = s
        .read(Read::range(key, b"", Consistency::Linearizable))
        .await
        .unwrap()
        .entries[0]
        .lease
        .unwrap();
    s.revoke(old).await.unwrap();
    let replacement = s.grant(20).await.unwrap();
    s.transact(
        vec![],
        vec![Mutation::Put {
            key: key.to_vec(),
            value: entry.serialize().unwrap(),
            lease: Some(replacement.id),
        }],
    )
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(900)).await;
    reg.shutdown().await.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        s.read(Read::range(key, b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .lease,
        Some(replacement.id)
    );
    c.cancel();
    assert!(get_ms_members(s.as_ref(), 58, "tso", &c).await.is_err());
}

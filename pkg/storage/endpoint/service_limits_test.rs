// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::keyspace_service_limit_path;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::sync::Arc;

// Go 1.26.8 encoding/json TestUnmarshal golden=true cases: Marshal output == input.
const GO_FLOATS: &[(f64, &[u8])] = &[
    (0.000001, b"0.000001"),
    (1e-7, b"1e-7"),
    (1e20, b"100000000000000000000"),
    (1e21, b"1e+21"),
    (-0.000001, b"-0.000001"),
    (-1e-7, b"-1e-7"),
    (-1e20, b"-100000000000000000000"),
    (-1e21, b"-1e+21"),
    (999999999999999900000.0, b"999999999999999900000"),
    (9007199254740992.0, b"9007199254740992"),
];
async fn rows(ep: &StorageEndpoint) -> Result<Vec<(u32, f64)>, EndpointError> {
    let mut rows = vec![];
    ep.load_service_limits(|id, value| rows.push((id, value)))
        .await?;
    Ok(rows)
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    assert!(rows(&ep).await.unwrap().is_empty());
    assert_eq!(ep.load_service_limit(42).await.unwrap(), 0.0);
    for &(value, bytes) in GO_FLOATS {
        ep.save_service_limit(42, value).await.unwrap();
        assert_eq!(
            base.load(keyspace_service_limit_path(42).as_bytes())
                .await
                .unwrap(),
            bytes
        );
        assert_eq!(ep.load_service_limit(42).await.unwrap(), value);
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            ep.save_service_limit(42, value).await,
            Err(EndpointError::JsonMarshal(_))
        ));
        assert_eq!(
            ep.load_service_limit(42).await.unwrap(),
            GO_FLOATS.last().unwrap().0
        );
    }
    for (value, bytes) in [
        (0.0, b"0".as_slice()),
        (-0.0, b"-0"),
        (1.0, b"1"),
        (1.25, b"1.25"),
        (-2.5, b"-2.5"),
    ] {
        ep.save_service_limit(42, value).await.unwrap();
        assert_eq!(
            base.load(keyspace_service_limit_path(42).as_bytes())
                .await
                .unwrap(),
            bytes
        );
        assert_eq!(
            ep.load_service_limit(42).await.unwrap().to_bits(),
            value.to_bits()
        );
    }
    for raw in [b"".as_slice(), b"null", b" \nnull "] {
        base.save(keyspace_service_limit_path(42).as_bytes(), raw)
            .await
            .unwrap();
        assert_eq!(ep.load_service_limit(42).await.unwrap(), 0.0);
    }
    for raw in [
        b"{".as_slice(),
        b"true",
        b"\"1.25\"",
        b"[]",
        b"{}",
        b"NaN",
        b"1e400",
        b"1 2",
        b"\xff",
        b" ",
        b"01",
    ] {
        base.save(keyspace_service_limit_path(42).as_bytes(), raw)
            .await
            .unwrap();
        assert!(matches!(
            ep.load_service_limit(42).await,
            Err(EndpointError::JsonUnmarshal(_))
        ));
    }
    // Invalid physical rows are skipped; null is a valid zero, empty bytes are not.
    for (suffix, raw) in [
        ("", b"1".as_slice()),
        ("+1", b"1"),
        ("-1", b"1"),
        ("4294967296", b"1"),
        ("1/x", b"1"),
        ("bad", b"1"),
        ("42", b"{"),
        ("43", b""),
        ("44", b"true"),
    ] {
        base.save(
            format!("resource_group/keyspace/service_limits/{suffix}").as_bytes(),
            raw,
        )
        .await
        .unwrap();
    }
    base.save(b"resource_group/keyspace/service_limits/\xff", b"1")
        .await
        .unwrap();
    for key in [
        b"resource_group/keyspace/service_limits".as_slice(),
        b"resource_group/keyspace/service_limits0/1",
        b"resource_group/controller",
        b"resource_group/keyspace/states/1/x",
    ] {
        base.save(key, b"9").await.unwrap();
    }
    let mut expected = vec![];
    for id in (1000..1201).chain([0, 1, u32::MAX - 1, u32::MAX]) {
        ep.save_service_limit(id, 1.25).await.unwrap();
        expected.push((id, 1.25));
    }
    base.save(b"resource_group/keyspace/service_limits/0007", b"null")
        .await
        .unwrap();
    expected.push((7, 0.0));
    expected.sort_by_key(|r| {
        if r.0 == 7 {
            "0007".to_string()
        } else {
            r.0.to_string()
        }
    });
    assert_eq!(rows(&ep).await.unwrap(), expected);
    ep.save_service_limit(1, 2.5).await.unwrap();
    assert_eq!(ep.load_service_limit(1).await.unwrap(), 2.5);
    assert_eq!(ep.load_service_limit(0).await.unwrap(), 1.25);
}
#[tokio::test]
async fn service_limits_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn service_limits_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    let before = rows(&ep).await.unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(rows(&ep).await.unwrap(), before);
    // Isolate the disconnect page boundary from malformed rows in the contract.
    let store = fixture.store().await;
    let snapshot = store
        .read(astersql_pd_storage_kv::Read::prefix(
            b"resource_group/keyspace/service_limits/",
            astersql_pd_storage_kv::Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let base = Base::new(&store);
    for entry in snapshot.entries {
        base.remove(&entry.key).await.unwrap();
    }
    for id in 0..201 {
        ep.save_service_limit(id, 1.25).await.unwrap();
    }
    let mut fixture = Some(fixture);
    let mut delivered = 0;
    let result = ep
        .load_service_limits(|_, _| {
            delivered += 1;
            if delivered == 100 {
                drop(fixture.take());
            }
        })
        .await;
    assert!(matches!(result, Err(EndpointError::Store(_))));
    assert_eq!(delivered, 100);
    assert!(matches!(
        ep.load_service_limit(1).await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.save_service_limit(1, 1.25).await,
        Err(EndpointError::Store(_))
    ));
}

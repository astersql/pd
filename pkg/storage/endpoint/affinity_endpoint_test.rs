// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, CallbackTxnError, MemoryStore, MetaStore};
use serde::Serialize;
use std::sync::Arc;

// Saved actual Go encoding/json ExampleMarshal output, checked by its Go runner.
const GO_JSON: &[u8] = br#"{"ID":1,"Name":"Reds","Colors":["Crimson","Red","Ruby","Maroon"]}"#;
#[derive(Serialize)]
struct ColorGroup {
    #[serde(rename = "ID")]
    id: i64,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Colors")]
    colors: Vec<String>,
}
fn group() -> ColorGroup {
    ColorGroup {
        id: 1,
        name: "Reds".into(),
        colors: ["Crimson", "Red", "Ruby", "Maroon"]
            .map(str::to_owned)
            .to_vec(),
    }
}
struct Reject;
impl Serialize for Reject {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("unencodable value"))
    }
}
async fn rows(
    ep: &StorageEndpoint,
    p: &ClusterPaths,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, EndpointError> {
    let mut out = vec![];
    ep.load_all_affinity_groups(p, |k, v| out.push((k.to_vec(), v.to_vec())))
        .await?;
    Ok(out)
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let p = ClusterPaths::new(42);
    let base = Base::new(store.as_ref());
    assert!(rows(&ep, &p).await.unwrap().is_empty());
    assert!(
        ep.load_affinity_group(&p, "missing")
            .await
            .unwrap()
            .is_empty()
    );
    ep.run_in_txn(async |txn| {
        ep.save_affinity_group(txn, &p, "group1", &group())?;
        ep.save_affinity_group(txn, &p, "group10", &1)?;
        assert!(ep.load_affinity_group(&p, "group1").await?.is_empty());
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    assert_eq!(ep.load_affinity_group(&p, "group1").await.unwrap(), GO_JSON);
    assert_eq!(
        rows(&ep, &p).await.unwrap(),
        vec![
            (b"group1".to_vec(), GO_JSON.to_vec()),
            (b"group10".to_vec(), b"1".to_vec())
        ]
    );
    let failed = ep
        .run_in_txn(async |txn| {
            ep.delete_affinity_group(txn, &p, "group1")?;
            ep.save_affinity_group(txn, &p, "new", &1)?;
            ep.save_affinity_group(txn, &p, "bad", &Reject)?;
            Ok::<_, EndpointError>(())
        })
        .await;
    assert!(matches!(
        failed,
        Err(CallbackTxnError::Callback(EndpointError::JsonMarshal(_)))
    ));
    assert_eq!(ep.load_affinity_group(&p, "group1").await.unwrap(), GO_JSON);
    assert!(ep.load_affinity_group(&p, "new").await.unwrap().is_empty());
    let failed = ep
        .run_in_txn(async |txn| {
            txn.load(p.affinity_group_path("group1").as_bytes()).await?;
            ep.delete_affinity_group(txn, &p, "group10")?;
            ep.save_affinity_group(txn, &p, "new", &1)?;
            base.save(p.affinity_group_path("group1").as_bytes(), b"external")
                .await?;
            Ok::<_, EndpointError>(())
        })
        .await;
    assert!(matches!(failed, Err(CallbackTxnError::Conflict)));
    assert_eq!(ep.load_affinity_group(&p, "group10").await.unwrap(), b"1");
    assert!(ep.load_affinity_group(&p, "new").await.unwrap().is_empty());
    for _ in 0..2 {
        ep.run_in_txn(async |txn| {
            ep.delete_affinity_group(txn, &p, "group1")?;
            Ok::<_, EndpointError>(())
        })
        .await
        .unwrap();
    }
    assert_eq!(ep.load_affinity_group(&p, "group10").await.unwrap(), b"1");
    ep.run_in_txn(async |txn| {
        ep.delete_affinity_group(txn, &p, "group10")?;
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    for key in [
        "/pd/42/affinity_groups",
        "/pd/42/affinity_groups0/a",
        "/pd/43/affinity_groups/a",
        "/pd/42/rules/a",
    ] {
        base.save(key.as_bytes(), b"outside").await.unwrap();
    }
    let mut expected = vec![];
    for n in 0..201 {
        let name = format!("g{n:03}");
        base.save(p.affinity_group_path(&name).as_bytes(), GO_JSON)
            .await
            .unwrap();
        expected.push((name.into_bytes(), GO_JSON.to_vec()));
    }
    for (name, value) in [
        (b"".as_slice(), b"".as_slice()),
        (b"g098\0", b"{bad json"),
        (b"/a//../b/", b"\xff\0"),
        (b"\xff", b"null"),
    ] {
        let mut key = p.affinity_groups_prefix().into_bytes();
        key.extend_from_slice(name);
        base.save(&key, value).await.unwrap();
        expected.push((name.to_vec(), value.to_vec()));
    }
    expected.sort();
    assert_eq!(rows(&ep, &p).await.unwrap(), expected);
    assert!(ep.load_affinity_group(&p, "").await.unwrap().is_empty());
    assert_eq!(
        ep.load_affinity_group(&p, "g098\0").await.unwrap(),
        b"{bad json"
    );
    assert_eq!(
        base.load(b"/pd/43/affinity_groups/a").await.unwrap(),
        b"outside"
    );
}
#[test]
fn affinity_endpoint_go_bytes() {
    assert_eq!(serde_json::to_vec(&group()).unwrap(), GO_JSON);
}
#[tokio::test]
async fn affinity_endpoint_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn affinity_endpoint_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let p = ClusterPaths::new(99);
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    ep.run_in_txn(async |txn| {
        ep.save_affinity_group(txn, &p, "persist", &group())?;
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        rows(&ep, &p).await.unwrap(),
        vec![(b"persist".to_vec(), GO_JSON.to_vec())]
    );
    let mut fixture = Some(fixture);
    let mut delivered = 0;
    let failure = ep
        .load_all_affinity_groups(&ClusterPaths::new(42), |_, _| {
            delivered += 1;
            if delivered == 100 {
                drop(fixture.take());
            }
        })
        .await;
    assert!(matches!(failure, Err(EndpointError::Store(_))));
    assert_eq!(delivered, 100);
    assert!(matches!(
        ep.load_affinity_group(&p, "persist").await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.run_in_txn(async |txn| {
            ep.delete_affinity_group(txn, &p, "persist")?;
            Ok::<_, EndpointError>(())
        })
        .await,
        Err(CallbackTxnError::Store(_))
    ));
}

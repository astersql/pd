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
    kind: usize,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, EndpointError> {
    let mut out = vec![];
    let mut collect = |k: &[u8], v: &[u8]| out.push((k.to_vec(), v.to_vec()));
    match kind {
        0 => ep.load_rules(p, &mut collect).await?,
        1 => ep.load_rule_groups(p, &mut collect).await?,
        _ => ep.load_region_rules(p, &mut collect).await?,
    }
    Ok(out)
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let p = ClusterPaths::new(42);
    let base = Base::new(store.as_ref());
    for kind in 0..3 {
        assert!(rows(&ep, &p, kind).await.unwrap().is_empty());
    }
    assert!(ep.load_rule(&p, "missing").await.unwrap().is_empty());
    ep.run_in_txn(async |txn| {
        ep.save_rule(txn, &p, "same", &group())?;
        ep.save_rule_group(txn, &p, "same", &1)?;
        ep.save_region_rule(txn, &p, "same", &Option::<i64>::None)?;
        // Buffered writes are invisible until commit.
        assert!(ep.load_rule(&p, "same").await?.is_empty());
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    assert_eq!(ep.load_rule(&p, "same").await.unwrap(), GO_JSON);
    for (kind, value) in [GO_JSON, b"1", b"null"].into_iter().enumerate() {
        assert_eq!(
            rows(&ep, &p, kind).await.unwrap(),
            vec![(b"same".to_vec(), value.to_vec())]
        );
    }
    // All three prefixes survive callback/encoding failure without any partial write.
    let failed = ep
        .run_in_txn(async |txn| {
            ep.delete_rule(txn, &p, "same")?;
            ep.save_rule_group(txn, &p, "same", &2)?;
            ep.delete_region_rule(txn, &p, "same")?;
            ep.save_rule(txn, &p, "bad", &Reject)?;
            Ok::<_, EndpointError>(())
        })
        .await;
    assert!(matches!(
        failed,
        Err(CallbackTxnError::Callback(EndpointError::JsonMarshal(_)))
    ));
    assert_eq!(ep.load_rule(&p, "same").await.unwrap(), GO_JSON);
    assert_eq!(rows(&ep, &p, 1).await.unwrap()[0].1, b"1");
    assert_eq!(rows(&ep, &p, 2).await.unwrap()[0].1, b"null");
    // Force a real compare failure between live read and commit.
    let failed = ep
        .run_in_txn(async |txn| {
            txn.load(p.rule_key_path("same").as_bytes()).await?;
            ep.delete_rule_group(txn, &p, "same")?;
            ep.delete_region_rule(txn, &p, "same")?;
            ep.save_rule(txn, &p, "new", &1)?;
            base.save(p.rule_key_path("same").as_bytes(), b"external")
                .await?;
            Ok::<_, EndpointError>(())
        })
        .await;
    assert!(matches!(failed, Err(CallbackTxnError::Conflict)));
    assert!(ep.load_rule(&p, "new").await.unwrap().is_empty());
    assert_eq!(rows(&ep, &p, 1).await.unwrap()[0].1, b"1");
    assert_eq!(rows(&ep, &p, 2).await.unwrap()[0].1, b"null");
    let duplicate = ep
        .run_in_txn(async |txn| {
            ep.delete_rule(txn, &p, "same")?;
            ep.delete_rule(txn, &p, "same")?;
            Ok::<_, EndpointError>(())
        })
        .await;
    assert!(matches!(duplicate, Err(CallbackTxnError::Store(_))));
    assert_eq!(ep.load_rule(&p, "same").await.unwrap(), b"external");
    for _ in 0..2 {
        ep.run_in_txn(async |txn| {
            ep.delete_rule(txn, &p, "same")?;
            ep.delete_rule_group(txn, &p, "same")?;
            ep.delete_region_rule(txn, &p, "same")?;
            Ok::<_, EndpointError>(())
        })
        .await
        .unwrap();
    }
    for kind in 0..3 {
        assert!(rows(&ep, &p, kind).await.unwrap().is_empty());
    }
    // Adjacent keys and other cluster must remain outside all lists.
    for key in [
        "/pd/42/rules",
        "/pd/42/rules0/a",
        "/pd/42/rule_group0/a",
        "/pd/42/region_label0/a",
        "/pd/43/rules/a",
    ] {
        base.save(key.as_bytes(), b"outside").await.unwrap();
    }
    // More than two pages, including the page cursor + NUL edge and raw invalid bytes.
    for prefix in [
        p.rules_path_prefix(),
        p.rule_group_path_prefix(),
        p.region_label_path_prefix(),
    ] {
        let mut expected = vec![];
        for n in 0..201 {
            let name = format!("r{n:03}").into_bytes();
            let mut key = prefix.as_bytes().to_vec();
            key.extend_from_slice(&name);
            base.save(&key, GO_JSON).await.unwrap();
            expected.push((name, GO_JSON.to_vec()));
        }
        for name in [b"".as_slice(), b"r098\0", b"/a//../b/", b"\xff"] {
            let mut key = prefix.as_bytes().to_vec();
            key.extend_from_slice(name);
            base.save(&key, b"\xff\0{bad json").await.unwrap();
            expected.push((name.to_vec(), b"\xff\0{bad json".to_vec()));
        }
        expected.sort();
        let kind = if prefix == p.rules_path_prefix() {
            0
        } else if prefix == p.rule_group_path_prefix() {
            1
        } else {
            2
        };
        assert_eq!(rows(&ep, &p, kind).await.unwrap(), expected);
    }
    assert_eq!(base.load(b"/pd/43/rules/a").await.unwrap(), b"outside");
}
#[test]
fn rule_endpoint_go_bytes() {
    assert_eq!(serde_json::to_vec(&group()).unwrap(), GO_JSON);
}
#[tokio::test]
async fn rule_endpoint_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn rule_endpoint_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let p = ClusterPaths::new(99);
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    ep.run_in_txn(async |txn| {
        ep.save_rule(txn, &p, "persist", &group())?;
        ep.save_rule_group(txn, &p, "persist", &1)?;
        ep.save_region_rule(txn, &p, "persist", &Option::<i64>::None)?;
        Ok::<_, EndpointError>(())
    })
    .await
    .unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    for (kind, bytes) in [GO_JSON, b"1", b"null"].into_iter().enumerate() {
        assert_eq!(
            rows(&ep, &p, kind).await.unwrap(),
            vec![(b"persist".to_vec(), bytes.to_vec())]
        );
    }
    // A later-page IO failure must preserve already delivered callbacks.
    let pagination = ClusterPaths::new(42);
    let mut fixture = Some(fixture);
    let mut delivered = 0;
    let failure = ep
        .load_rules(&pagination, |_, _| {
            delivered += 1;
            if delivered == 100 {
                drop(fixture.take());
            }
        })
        .await;
    assert!(matches!(failure, Err(EndpointError::Store(_))));
    assert_eq!(delivered, 100);
    assert!(fixture.is_none());
    for kind in 0..3 {
        assert!(matches!(
            rows(&ep, &p, kind).await,
            Err(EndpointError::Store(_))
        ));
    }
    assert!(matches!(
        ep.load_rule(&p, "persist").await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.run_in_txn(async |txn| {
            ep.delete_rule(txn, &p, "persist")?;
            ep.delete_rule_group(txn, &p, "persist")?;
            ep.delete_region_rule(txn, &p, "persist")?;
            Ok::<_, EndpointError>(())
        })
        .await,
        Err(CallbackTxnError::Store(_))
    ));
}

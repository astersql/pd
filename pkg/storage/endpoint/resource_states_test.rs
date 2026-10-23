// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::keyspace_resource_group_state_path;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use serde::Serialize;

use std::sync::Arc;

// Actual Go encoding/json ExampleMarshal output, saved in batch 21 evidence.
const GO_JSON: &[u8] = br#"{"ID":1,"Name":"Reds","Colors":["Crimson","Red","Ruby","Maroon"]}"#;
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Group {
    #[serde(rename = "ID")]
    id: u64,
    name: &'static str,
    colors: [&'static str; 4],
}
fn group() -> Group {
    Group {
        id: 1,
        name: "Reds",
        colors: ["Crimson", "Red", "Ruby", "Maroon"],
    }
}
struct Reject;
impl Serialize for Reject {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("unencodable state"))
    }
}
async fn rows(ep: &StorageEndpoint) -> Result<Vec<(u32, Vec<u8>, Vec<u8>)>, EndpointError> {
    let mut out = vec![];
    ep.load_resource_group_states(|id, name, value| out.push((id, name.to_vec(), value.to_vec())))
        .await?;
    Ok(out)
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    assert!(rows(&ep).await.unwrap().is_empty());
    assert_eq!(ep.load_controller_config().await.unwrap(), b"");
    ep.save_controller_config(&group()).await.unwrap();
    assert_eq!(ep.load_controller_config().await.unwrap(), GO_JSON);
    assert_eq!(
        base.load(b"resource_group/controller").await.unwrap(),
        GO_JSON
    );
    assert!(matches!(
        ep.save_controller_config(&Reject).await,
        Err(EndpointError::JsonMarshal(_))
    ));
    assert_eq!(ep.load_controller_config().await.unwrap(), GO_JSON);
    for raw in [
        b"".as_slice(),
        b"{",
        b"null",
        b" \n{ \"x\": 1 } ",
        b"\xff\0",
    ] {
        base.save(b"resource_group/controller", raw).await.unwrap();
        assert_eq!(ep.load_controller_config().await.unwrap(), raw);
    }
    ep.save_controller_config(&group()).await.unwrap();
    for id in [u32::MAX, 0] {
        ep.save_resource_group_states(id, "reject", &1)
            .await
            .unwrap();
        assert!(matches!(
            ep.save_resource_group_states(id, "reject", &Reject).await,
            Err(EndpointError::JsonMarshal(_))
        ));
        assert_eq!(
            base.load(keyspace_resource_group_state_path(id, "reject").as_bytes())
                .await
                .unwrap(),
            b"1"
        );
        ep.delete_resource_group_states(id, "reject").await.unwrap();
    }
    let wire = GO_JSON.to_vec();
    let encoded = group();
    for id in [u32::MAX, 0, 1, u32::MAX - 1] {
        ep.save_resource_group_states(id, "group", &encoded)
            .await
            .unwrap();
        assert_eq!(
            base.load(keyspace_resource_group_state_path(id, "group").as_bytes())
                .await
                .unwrap(),
            wire
        );
    }
    assert_eq!(
        rows(&ep).await.unwrap(),
        vec![
            (u32::MAX, b"group".to_vec(), wire.clone()),
            (0, b"group".to_vec(), wire.clone()),
            (1, b"group".to_vec(), wire.clone()),
            (u32::MAX - 1, b"group".to_vec(), wire.clone()),
        ]
    );
    for id in [u32::MAX, 0, 1, u32::MAX - 1] {
        ep.save_resource_group_states(id, "group", &Option::<u8>::None)
            .await
            .unwrap();
        assert_eq!(
            base.load(keyspace_resource_group_state_path(id, "group").as_bytes())
                .await
                .unwrap(),
            b"null"
        );
        ep.delete_resource_group_states(id, "group").await.unwrap();
        ep.delete_resource_group_states(id, "group").await.unwrap();
    }
    let mut expected = vec![];
    // Both prefixes cross two page boundaries, including embedded NUL continuation.
    for id in [u32::MAX, 7] {
        for i in 0..201 {
            let name = format!("g{i:03}");
            ep.save_resource_group_states(id, &name, &encoded)
                .await
                .unwrap();
            expected.push((id, name.into_bytes(), wire.clone()));
        }
        for (name, value) in [
            ("", b"".as_slice()),
            ("g098\0", b"\xff"),
            ("/a//../b/", b"\0"),
        ] {
            base.save(
                keyspace_resource_group_state_path(id, name).as_bytes(),
                value,
            )
            .await
            .unwrap();
            expected.push((id, name.as_bytes().to_vec(), value.to_vec()));
        }
    }
    for key in [
        "resource_group/states",
        "resource_group/states0/x",
        "resource_group/settings/x",
        "resource_group/keyspace/settings/7/x",
        "resource_group/keyspace/states/invalid/x",
        "resource_group/keyspace/states/4294967296/x",
        "resource_group/keyspace/states/7",
        "resource_group/keyspace/states//x",
        "resource_group/keyspace/states/+7/x",
    ] {
        base.save(key.as_bytes(), b"outside").await.unwrap();
    }
    for (key, id, name) in [
        (
            b"resource_group/states/\xff".as_slice(),
            u32::MAX,
            b"\xff".as_slice(),
        ),
        (b"resource_group/keyspace/states/7/\xff", 7, b"\xff"),
        (b"resource_group/keyspace/states/0008/x/y", 8, b"x/y"),
    ] {
        base.save(key, b"raw").await.unwrap();
        expected.push((id, name.to_vec(), b"raw".to_vec()));
    }
    // Legacy first; keyspace records are sorted by physical key, not numeric ID.
    let mut legacy: Vec<_> = expected
        .iter()
        .filter(|r| r.0 == u32::MAX)
        .cloned()
        .collect();
    legacy.sort_by(|a, b| a.1.cmp(&b.1));
    let mut keyed: Vec<_> = expected.into_iter().filter(|r| r.0 != u32::MAX).collect();
    keyed.sort_by_key(|r| {
        if r.0 == 8 {
            b"0008/x/y".to_vec()
        } else {
            let mut k = b"7/".to_vec();
            k.extend_from_slice(&r.1);
            k
        }
    });
    legacy.extend(keyed);
    assert_eq!(rows(&ep).await.unwrap(), legacy);
    assert_eq!(ep.load_controller_config().await.unwrap(), GO_JSON);
    // Deletion must leave an adjacent name and nested key intact.
    ep.delete_resource_group_states(7, "g001").await.unwrap();
    assert_eq!(
        base.load(b"resource_group/keyspace/states/7/g002")
            .await
            .unwrap(),
        wire
    );
    assert_eq!(
        base.load(b"resource_group/keyspace/states/0008/x/y")
            .await
            .unwrap(),
        b"raw"
    );
}
#[test]
fn resource_states_go_bytes() {
    assert_eq!(serde_json::to_vec(&group()).unwrap(), GO_JSON);
}
#[tokio::test]
async fn resource_states_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn resource_states_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    let before = rows(&ep).await.unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(rows(&ep).await.unwrap(), before);
    assert_eq!(ep.load_controller_config().await.unwrap(), GO_JSON);
    let mut fixture = Some(fixture);
    let mut delivered = 0;
    let result = ep
        .load_resource_group_states(|_, _, _| {
            delivered += 1;
            if delivered == 100 {
                drop(fixture.take());
            }
        })
        .await;
    assert!(matches!(result, Err(EndpointError::Store(_))));
    assert_eq!(delivered, 100);
    assert!(matches!(
        ep.load_controller_config().await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.save_controller_config(&group()).await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.save_resource_group_states(7, "x", &group()).await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.delete_resource_group_states(7, "x").await,
        Err(EndpointError::Store(_))
    ));
}

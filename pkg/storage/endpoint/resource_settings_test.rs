// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::keyspace_resource_group_setting_path;
use astersql_pd_proto::pdpb::RequestHeader;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use prost::Message;
use std::sync::Arc;

fn go_wire() -> Vec<u8> {
    // Existing actual gogo/protobuf output from scripts/rust-proto-oracle.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../proto/wire-fixtures.json")).unwrap();
    let hex = fixture["request_header"].as_str().unwrap();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
async fn rows(ep: &StorageEndpoint) -> Result<Vec<(u32, Vec<u8>, Vec<u8>)>, EndpointError> {
    let mut out = vec![];
    ep.load_resource_group_settings(|id, name, value| {
        out.push((id, name.to_vec(), value.to_vec()))
    })
    .await?;
    Ok(out)
}
async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    assert!(rows(&ep).await.unwrap().is_empty());
    let wire = go_wire();
    // Go accepts any proto.Message, so the existing RequestHeader fixture is valid here.
    let encoded = RequestHeader {
        cluster_id: 42,
        sender_id: 7,
        caller_id: "oracle".into(),
        caller_component: "pd".into(),
    }
    .encode_to_vec();
    assert_eq!(encoded, wire);
    for id in [u32::MAX, 0, 1, u32::MAX - 1] {
        ep.save_resource_group_setting(id, "group", &encoded)
            .await
            .unwrap();
        assert_eq!(
            base.load(keyspace_resource_group_setting_path(id, "group").as_bytes())
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
        ep.save_resource_group_setting(id, "group", b"\xff\0")
            .await
            .unwrap();
        assert_eq!(
            base.load(keyspace_resource_group_setting_path(id, "group").as_bytes())
                .await
                .unwrap(),
            b"\xff\0"
        );
        ep.delete_resource_group_setting(id, "group").await.unwrap();
        ep.delete_resource_group_setting(id, "group").await.unwrap();
    }
    let mut expected = vec![];
    // Both prefixes cross two page boundaries, including embedded NUL continuation.
    for id in [u32::MAX, 7] {
        for i in 0..201 {
            let name = format!("g{i:03}");
            ep.save_resource_group_setting(id, &name, &wire)
                .await
                .unwrap();
            expected.push((id, name.into_bytes(), wire.clone()));
        }
        for (name, value) in [
            ("", b"".as_slice()),
            ("g098\0", b"\xff"),
            ("/a//../b/", b"\0"),
        ] {
            ep.save_resource_group_setting(id, name, value)
                .await
                .unwrap();
            expected.push((id, name.as_bytes().to_vec(), value.to_vec()));
        }
    }
    for key in [
        "resource_group/settings",
        "resource_group/settings0/x",
        "resource_group/states/x",
        "resource_group/keyspace/states/7/x",
        "resource_group/keyspace/settings/invalid/x",
        "resource_group/keyspace/settings/4294967296/x",
        "resource_group/keyspace/settings/7",
        "resource_group/keyspace/settings//x",
        "resource_group/keyspace/settings/+7/x",
    ] {
        base.save(key.as_bytes(), b"outside").await.unwrap();
    }
    for (key, id, name) in [
        (
            b"resource_group/settings/\xff".as_slice(),
            u32::MAX,
            b"\xff".as_slice(),
        ),
        (b"resource_group/keyspace/settings/7/\xff", 7, b"\xff"),
        (b"resource_group/keyspace/settings/0008/x/y", 8, b"x/y"),
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
    // Deletion must leave an adjacent name and nested key intact.
    ep.delete_resource_group_setting(7, "g001").await.unwrap();
    assert_eq!(
        base.load(b"resource_group/keyspace/settings/7/g002")
            .await
            .unwrap(),
        wire
    );
    assert_eq!(
        base.load(b"resource_group/keyspace/settings/0008/x/y")
            .await
            .unwrap(),
        b"raw"
    );
}
#[test]
fn resource_settings_go_bytes() {
    assert_eq!(
        RequestHeader {
            cluster_id: 42,
            sender_id: 7,
            caller_id: "oracle".into(),
            caller_component: "pd".into()
        }
        .encode_to_vec(),
        go_wire()
    );
}
#[tokio::test]
async fn resource_settings_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn resource_settings_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    let before = rows(&ep).await.unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(rows(&ep).await.unwrap(), before);
    let mut fixture = Some(fixture);
    let mut delivered = 0;
    let result = ep
        .load_resource_group_settings(|_, _, _| {
            delivered += 1;
            if delivered == 100 {
                drop(fixture.take());
            }
        })
        .await;
    assert!(matches!(result, Err(EndpointError::Store(_))));
    assert_eq!(delivered, 100);
    assert!(matches!(
        ep.save_resource_group_setting(7, "x", &go_wire()).await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.delete_resource_group_setting(7, "x").await,
        Err(EndpointError::Store(_))
    ));
}

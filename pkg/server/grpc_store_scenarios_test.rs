// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use serde_json::{Value, json};
type Client = PdClient<tonic::transport::Channel>;
pub(crate) fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
pub(crate) fn meta(id: u64, address: &str) -> metapb::Store {
    metapb::Store {
        id,
        address: address.into(),
        version: "8.5.0".into(),
        node_state: 1,
        ..Default::default()
    }
}
pub(crate) async fn put(
    client: &mut Client,
    id: u64,
    store: metapb::Store,
) -> pdpb::PutStoreResponse {
    let result = client
        .put_store(pdpb::PutStoreRequest {
            header: header(id),
            store: Some(store),
        })
        .await;
    assert!(result.is_ok(), "PutStore must be served: {result:?}");
    result.unwrap().into_inner()
}
pub(crate) async fn get(client: &mut Client, id: u64, store_id: u64) -> pdpb::GetStoreResponse {
    client
        .get_store(pdpb::GetStoreRequest {
            header: header(id),
            store_id,
        })
        .await
        .unwrap()
        .into_inner()
}
pub(crate) async fn all(client: &mut Client, id: u64, exclude: bool) -> Vec<metapb::Store> {
    let r = client
        .get_all_stores(pdpb::GetAllStoresRequest {
            header: header(id),
            exclude_tombstone_stores: exclude,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
    let mut stores = r.stores;
    stores.sort_by_key(|s| s.id);
    stores
}
pub(crate) async fn observe(client: &mut Client, id: u64) -> Value {
    let unboot = client
        .get_all_stores(pdpb::GetAllStoresRequest {
            header: header(id),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        unboot.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    let unboot = put(client, id, meta(4, "store:4")).await;
    assert_eq!(
        unboot.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    let reply = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header(id),
            store: Some(meta(1, "bootstrap:1")),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![metapb::Peer {
                    id: 3,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(reply.header.unwrap().error.is_none());
    let mut original = meta(4, "store:4");
    original.peer_address = "peer:4".into();
    original.status_address = "status:4".into();
    original.git_hash = "abcdef".into();
    original.deploy_path = "/srv/雪\n".into();
    original.start_timestamp = 123;
    original.txn_protocol_version_range = Some(metapb::TxnProtocolVersionRange { min: 1, max: 3 });
    original.labels = vec![
        metapb::StoreLabel {
            key: "zone".into(),
            value: "old".into(),
        },
        metapb::StoreLabel {
            key: "rack".into(),
            value: "keep".into(),
        },
    ];
    let reply = put(client, id, original.clone()).await;
    assert!(reply.header.unwrap().error.is_none());
    assert_eq!(
        reply.replication_status.unwrap().mode,
        astersql_pd_proto::replication_modepb::ReplicationMode::Majority as i32
    );
    assert_eq!(get(client, id, 4).await.store, Some(original.clone()));
    let invalid = put(client, id, metapb::Store::default())
        .await
        .header
        .unwrap()
        .error
        .unwrap();
    assert_eq!(invalid.message, "invalid put store ");
    assert_eq!(invalid.r#type, pdpb::ErrorType::Unknown as i32);
    let absent = client
        .put_store(pdpb::PutStoreRequest {
            header: header(id),
            store: None,
        })
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .error
        .unwrap();
    assert_eq!(absent.message, "invalid put store <nil>");
    let mut errors = Vec::new();
    for (address, peer, prefix) in [
        ("store:4", "", "duplicated store address:"),
        ("peer:4", "", "duplicated store address:"),
        ("unique", "store:4", "duplicated store peer address:"),
        ("unique", "peer:4", "duplicated store peer address:"),
    ] {
        let mut store = meta(5, address);
        store.peer_address = peer.into();
        let reply = put(client, id, store).await;
        let h = reply.header.unwrap();
        let error = h.error.unwrap();
        assert_eq!(h.cluster_id, id);
        assert_eq!(error.r#type, pdpb::ErrorType::Unknown as i32);
        assert!(error.message.starts_with(prefix), "{}", error.message);
        assert!(reply.replication_status.is_none());
        assert!(get(client, id, 5).await.store.is_none());
        errors.push(json!([error.r#type, error.message]));
    }
    let mut update = meta(4, "store:new");
    update.labels = vec![metapb::StoreLabel {
        key: "ZONE".into(),
        value: "new".into(),
    }];
    update.state = 2;
    update.node_state = 3;
    update.physically_destroyed = true;
    assert!(
        put(client, id, update)
            .await
            .header
            .unwrap()
            .error
            .is_none()
    );
    let updated = get(client, id, 4).await.store.unwrap();
    assert_eq!(
        (
            updated.state,
            updated.node_state,
            updated.physically_destroyed
        ),
        (0, 1, false)
    );
    assert_eq!(updated.labels[0].value, "new");
    assert_eq!(updated.labels[1].value, "keep");
    let mut removed = meta(8, "retired");
    removed.node_state = 3;
    removed.state = 2;
    removed.last_heartbeat = i64::MAX;
    assert!(
        put(client, id, removed.clone())
            .await
            .header
            .unwrap()
            .error
            .is_none()
    );
    let error = put(client, id, meta(8, "resurrect"))
        .await
        .header
        .unwrap()
        .error
        .unwrap();
    assert_eq!(
        (error.r#type, error.message.as_str()),
        (pdpb::ErrorType::StoreTombstone as i32, "store is tombstone")
    );
    assert_eq!(get(client, id, 8).await.store, Some(removed));
    assert!(
        put(client, id, meta(10, "retired"))
            .await
            .header
            .unwrap()
            .error
            .is_none()
    );
    let mut destroyed = meta(9, "destroyed");
    destroyed.physically_destroyed = true;
    assert!(
        put(client, id, destroyed)
            .await
            .header
            .unwrap()
            .error
            .is_none()
    );
    assert!(
        put(client, id, meta(11, "destroyed"))
            .await
            .header
            .unwrap()
            .error
            .is_none()
    );
    let mut invalid = meta(12, "bad");
    invalid.labels = vec![metapb::StoreLabel {
        key: "engine".into(),
        value: "invalid".into(),
    }];
    let error = put(client, id, invalid)
        .await
        .header
        .unwrap()
        .error
        .unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::InvalidValue as i32);
    let missing = get(client, id, 999).await;
    assert_eq!(
        missing.header.unwrap().error.unwrap().message,
        "invalid store ID 999, not found"
    );
    assert!(missing.store.is_none() && missing.stats.is_none());
    let all = all(client, id, false).await;
    assert_eq!(
        all.iter().map(|s| s.id).collect::<Vec<_>>(),
        [1, 4, 8, 9, 10, 11]
    );
    assert_eq!(
        self::all(client, id, true)
            .await
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        [1, 4, 9, 10, 11]
    );
    assert_eq!(
        client
            .put_store(pdpb::PutStoreRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::FailedPrecondition
    );
    // Normalize unordered store enumeration by sorting; retain every metadata field.
    json!({"stores":format!("{all:?}"),"conflicts":errors,"engine_error":error.message})
}

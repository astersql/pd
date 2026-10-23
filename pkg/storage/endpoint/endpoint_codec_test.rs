// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_proto::pdpb::RequestHeader;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[path = "../kv/etcd_fixture_test.rs"]
#[allow(dead_code)]
pub(super) mod fixture;

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ColorGroup {
    #[serde(rename = "ID")]
    id: i64,
    name: String,
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
// Go 1.26.8 encoding/json ExampleMarshal: actual Output checked by go test.
const GO_JSON: &[u8] = br#"{"ID":1,"Name":"Reds","Colors":["Crimson","Red","Ruby","Maroon"]}"#;
struct Reject;
impl Serialize for Reject {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("unencodable value"))
    }
}
fn header() -> RequestHeader {
    RequestHeader {
        cluster_id: 42,
        sender_id: 7,
        caller_id: "oracle".into(),
        caller_component: "pd".into(),
    }
}
fn go_wire() -> Vec<u8> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../proto/wire-fixtures.json")).unwrap();
    let hex = fixture["request_header"].as_str().unwrap();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
async fn contract(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    assert_eq!(
        endpoint.load(b"/codec/missing").await.unwrap(),
        LoadState::Missing
    );
    assert_eq!(
        endpoint.load_json::<i64>(b"/codec/missing").await.unwrap(),
        0
    );
    assert!(
        protobuf::load::<RequestHeader>(&endpoint, b"/codec/missing")
            .await
            .unwrap()
            .is_none()
    );
    base.save(b"/codec/empty", b"").await.unwrap();
    assert_eq!(
        endpoint.load(b"/codec/empty").await.unwrap(),
        LoadState::Empty
    );
    assert_eq!(endpoint.load_json::<i64>(b"/codec/empty").await.unwrap(), 0);
    assert!(
        protobuf::load::<RequestHeader>(&endpoint, b"/codec/empty")
            .await
            .unwrap()
            .is_none()
    );
    endpoint.save_json(b"/codec/json", &group()).await.unwrap();
    assert_eq!(base.load(b"/codec/json").await.unwrap(), GO_JSON);
    assert_eq!(
        endpoint
            .load_json::<ColorGroup>(b"/codec/json")
            .await
            .unwrap(),
        group()
    );
    // TestGCStateJSONUtil's actual Go byte assertions (run read-only).
    for (value, bytes) in [
        (serde_json::json!(1), b"1".as_slice()),
        (serde_json::json!("str"), br#""str""#),
        (serde_json::Value::Null, b"null"),
    ] {
        endpoint.save_json(b"/codec/scalar", &value).await.unwrap();
        assert_eq!(base.load(b"/codec/scalar").await.unwrap(), bytes);
    }
    assert_eq!(
        endpoint
            .load_json::<Option<i64>>(b"/codec/scalar")
            .await
            .unwrap(),
        None
    );
    // Go TestUnmarshalNulls: null does not change a primitive zero value.
    assert_eq!(
        endpoint.load_json::<i64>(b"/codec/scalar").await.unwrap(),
        0
    );
    base.save(b"/codec/bad", b"{").await.unwrap();
    assert!(matches!(
        endpoint.load_json::<ColorGroup>(b"/codec/bad").await,
        Err(EndpointError::JsonUnmarshal(_))
    ));
    assert!(matches!(
        endpoint.save_json(b"/codec/json", &Reject).await,
        Err(EndpointError::JsonMarshal(_))
    ));
    assert_eq!(base.load(b"/codec/json").await.unwrap(), GO_JSON);
    protobuf::save(&endpoint, b"/codec/proto", &header())
        .await
        .unwrap();
    assert_eq!(base.load(b"/codec/proto").await.unwrap(), go_wire());
    assert_eq!(
        protobuf::load::<RequestHeader>(&endpoint, b"/codec/proto")
            .await
            .unwrap(),
        Some(header())
    );
    base.save(b"/codec/proto", b"\xff").await.unwrap();
    assert!(matches!(
        protobuf::load::<RequestHeader>(&endpoint, b"/codec/proto").await,
        Err(EndpointError::ProtoUnmarshal(_))
    ));
    // Invalid keys exercise a real backend error, not a mocked transaction.
    assert!(matches!(
        endpoint.save_json(b"", &1).await,
        Err(EndpointError::Store(_))
    ));
}
#[tokio::test]
async fn endpoint_codec_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn endpoint_codec_etcd() {
    let fixture = fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
}
#[test]
fn endpoint_codec_go_json_fixture() {
    assert_eq!(serde_json::to_vec(&group()).unwrap(), GO_JSON);
}

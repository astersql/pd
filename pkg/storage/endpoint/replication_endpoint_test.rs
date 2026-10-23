// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Real encoding/json ExampleMarshal output, previously verified by the Go test
// runner; see theme3 evidence, batch 21. No Rust encoder constructs this oracle.
const GO_JSON: &[u8] = br#"{"ID":1,"Name":"Reds","Colors":["Crimson","Red","Ruby","Maroon"]}"#;
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Config {
    #[serde(rename = "ID")]
    id: i64,
    name: String,
    colors: Vec<String>,
}
// Go structs accept JSON objects and ignore unknown fields. Serde's derived
// struct decoder also accepts positional arrays, so this schema adapter requires
// a map explicitly rather than treating [] as an empty Go struct.
impl<'de> Deserialize<'de> for Config {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Default, Deserialize)]
        #[serde(rename_all = "PascalCase", default)]
        struct Fields {
            #[serde(rename = "ID")]
            id: i64,
            name: String,
            colors: Vec<String>,
        }
        let object = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        let fields: Fields = serde_json::from_value(serde_json::Value::Object(object))
            .map_err(serde::de::Error::custom)?;
        Ok(Self {
            id: fields.id,
            name: fields.name,
            colors: fields.colors,
        })
    }
}
fn config() -> Config {
    Config {
        id: 1,
        name: "Reds".into(),
        colors: ["Crimson", "Red", "Ruby", "Maroon"]
            .map(str::to_owned)
            .into(),
    }
}
struct Reject;
impl Serialize for Reject {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("unencodable configuration"))
    }
}
async fn contract(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(42);
    let base = Base::new(store.as_ref());
    for mode in [
        "dr-auto-sync",
        "majority",
        "",
        "nested/name",
        "/a//../b/",
        "调度",
        "nul\0mode",
    ] {
        let key = paths.replication_mode_path(mode);
        assert_eq!(
            endpoint
                .load_replication_status::<Config>(&paths, mode)
                .await
                .unwrap(),
            None
        );
        base.save(key.as_bytes(), b"").await.unwrap();
        assert_eq!(
            endpoint
                .load_replication_status::<Config>(&paths, mode)
                .await
                .unwrap(),
            None
        );
        endpoint
            .save_replication_status(&paths, mode, &config())
            .await
            .unwrap();
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_JSON);
        assert_eq!(
            endpoint
                .load_replication_status::<Config>(&paths, mode)
                .await
                .unwrap(),
            Some(config())
        );
        base.save(key.as_bytes(), GO_JSON).await.unwrap();
        assert_eq!(
            endpoint
                .load_replication_status::<Config>(&paths, mode)
                .await
                .unwrap(),
            Some(config())
        );
        assert!(matches!(
            endpoint
                .save_replication_status(&paths, mode, &Reject)
                .await,
            Err(EndpointError::JsonMarshal(_))
        ));
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_JSON);
        for bad in [
            b"{".as_slice(),
            b"[]",
            b"true",
            b"{} {}",
            b"{\"ID\":\"bad\"}",
            b"\xff",
        ] {
            base.save(key.as_bytes(), bad).await.unwrap();
            assert!(matches!(
                endpoint
                    .load_replication_status::<Config>(&paths, mode)
                    .await,
                Err(EndpointError::JsonUnmarshal(_))
            ));
        }
        base.save(key.as_bytes(), b"{}").await.unwrap();
        assert_eq!(
            endpoint
                .load_replication_status::<Config>(&paths, mode)
                .await
                .unwrap(),
            Some(Config::default())
        );
        endpoint
            .save_replication_status(&paths, mode, &Option::<Config>::None)
            .await
            .unwrap();
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), b"null");
        assert_eq!(
            endpoint
                .load_replication_status::<Config>(&paths, mode)
                .await
                .unwrap(),
            Some(Config::default())
        );
        assert_eq!(
            endpoint
                .load_replication_status::<Option<Config>>(&paths, mode)
                .await
                .unwrap(),
            Some(None)
        );
        endpoint
            .save_replication_status(&paths, mode, &1_i64)
            .await
            .unwrap();
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), b"1");
        assert_eq!(
            endpoint
                .load_replication_status::<i64>(&paths, mode)
                .await
                .unwrap(),
            Some(1)
        );
        endpoint
            .save_replication_status(&paths, mode, &config())
            .await
            .unwrap();
    }
    endpoint
        .save_replication_status(&paths, "majority", &7_i64)
        .await
        .unwrap();
    assert_eq!(
        endpoint
            .load_replication_status::<i64>(&paths, "majority")
            .await
            .unwrap(),
        Some(7)
    );
    assert_eq!(
        endpoint
            .load_replication_status::<Config>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        Some(config())
    );
    assert_eq!(
        endpoint
            .load_replication_status::<Config>(&ClusterPaths::new(43), "dr-auto-sync")
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        base.load(b"/pd/42/replication_mode/dr-auto-sync")
            .await
            .unwrap(),
        GO_JSON
    );
}
#[tokio::test]
async fn replication_endpoint_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn replication_endpoint_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let paths = ClusterPaths::new(42);
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let endpoint = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        endpoint
            .load_replication_status::<Config>(&paths, "dr-auto-sync")
            .await
            .unwrap(),
        Some(config())
    );
    assert_eq!(
        endpoint
            .load_replication_status::<i64>(&paths, "majority")
            .await
            .unwrap(),
        Some(7)
    );
    drop(fixture);
    assert!(matches!(
        endpoint
            .load_replication_status::<Config>(&paths, "dr-auto-sync")
            .await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        endpoint
            .save_replication_status(&paths, "dr-auto-sync", &config())
            .await,
        Err(EndpointError::Store(_))
    ));
}
#[test]
fn replication_endpoint_go_bytes() {
    assert_eq!(serde_json::to_vec(&config()).unwrap(), GO_JSON);
}

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
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(42);
    for middleware in [false, true] {
        let key = if middleware {
            paths.service_middleware_path()
        } else {
            paths.config_path()
        };
        macro_rules! load {
            ($ty:ty) => {
                if middleware {
                    endpoint.load_service_middleware_config::<$ty>(&paths).await
                } else {
                    endpoint.load_config::<$ty>(&paths).await
                }
            };
        }
        macro_rules! save {
            ($value:expr) => {
                if middleware {
                    endpoint
                        .save_service_middleware_config(&paths, $value)
                        .await
                } else {
                    endpoint.save_config(&paths, $value).await
                }
            };
        }
        assert_eq!(load!(Config).unwrap(), None);
        base.save(key.as_bytes(), b"").await.unwrap();
        assert_eq!(load!(Config).unwrap(), None);
        save!(&config()).unwrap();
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), GO_JSON);
        assert_eq!(load!(Config).unwrap(), Some(config()));
        base.save(key.as_bytes(), GO_JSON).await.unwrap();
        assert_eq!(load!(Config).unwrap(), Some(config()));
        assert!(matches!(save!(&Reject), Err(EndpointError::JsonMarshal(_))));
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
                load!(Config),
                Err(EndpointError::JsonUnmarshal(_))
            ));
        }
        base.save(key.as_bytes(), b"{}").await.unwrap();
        assert_eq!(load!(Config).unwrap(), Some(Config::default()));
        // A nonempty JSON null is found, with a zero initialized model. In
        // contrast, physical absence and empty bytes above are not found.
        save!(&Option::<Config>::None).unwrap();
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), b"null");
        assert_eq!(load!(Config).unwrap(), Some(Config::default()));
        assert_eq!(load!(Option<Config>).unwrap(), Some(None));
        // Existing Go TestGCStateJSONUtil verifies these scalar bytes.
        save!(&1_i64).unwrap();
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), b"1");
        assert_eq!(load!(i64).unwrap(), Some(1));
        save!(&"str").unwrap();
        assert_eq!(base.load(key.as_bytes()).await.unwrap(), br#""str""#);
        assert_eq!(load!(String).unwrap(), Some("str".into()));
        save!(&config()).unwrap();
    }
    assert_eq!(
        endpoint
            .load_config::<Config>(&ClusterPaths::new(43))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        endpoint
            .load_service_middleware_config::<Config>(&ClusterPaths::new(43))
            .await
            .unwrap(),
        None
    );
    endpoint.save_config(&paths, &7_i64).await.unwrap();
    assert_eq!(
        endpoint
            .load_service_middleware_config::<Config>(&paths)
            .await
            .unwrap(),
        Some(config())
    );
    assert_eq!(endpoint.load_config::<i64>(&paths).await.unwrap(), Some(7));
}
#[tokio::test]
async fn config_endpoint_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn config_endpoint_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let endpoint = StorageEndpoint::new(Arc::new(fixture.store().await));
    let paths = ClusterPaths::new(99);
    endpoint.save_config(&paths, &config()).await.unwrap();
    endpoint
        .save_service_middleware_config(&paths, &config())
        .await
        .unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let endpoint = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        endpoint.load_config::<Config>(&paths).await.unwrap(),
        Some(config())
    );
    assert_eq!(
        endpoint
            .load_service_middleware_config::<Config>(&paths)
            .await
            .unwrap(),
        Some(config())
    );
    drop(fixture);
    assert!(matches!(
        endpoint.load_config::<Config>(&paths).await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        endpoint
            .load_service_middleware_config::<Config>(&paths)
            .await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        endpoint.save_config(&paths, &config()).await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        endpoint
            .save_service_middleware_config(&paths, &config())
            .await,
        Err(EndpointError::Store(_))
    ));
}
#[test]
fn config_endpoint_go_bytes() {
    assert_eq!(serde_json::to_vec(&config()).unwrap(), GO_JSON);
}

// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use prost::Message;
use tonic::transport::Channel;

pub async fn observe(client: &mut PdClient<Channel>) -> Vec<Vec<u8>> {
    client
        .store_global_config(pdpb::StoreGlobalConfigRequest {
            changes: vec![pdpb::GlobalConfigItem {
                name: "persist".into(),
                payload: vec![0, 255, 1],
                ..Default::default()
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    let response = client
        .store_global_config(pdpb::StoreGlobalConfigRequest {
            changes: vec![
                pdpb::GlobalConfigItem {
                    name: "binary".into(),
                    payload: vec![0, 255],
                    ..Default::default()
                },
                pdpb::GlobalConfigItem {
                    name: "legacy".into(),
                    value: "old".into(),
                    payload: b"ignored".to_vec(),
                    ..Default::default()
                },
            ],
            config_path: "/global/config/sub".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let loaded = client
        .load_global_config(pdpb::LoadGlobalConfigRequest {
            names: vec!["legacy".into(), "missing".into(), "binary".into()],
            config_path: "/global/config/sub/".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(loaded.revision, 0);
    assert_eq!(loaded.items[0].payload, b"old");
    assert_eq!(
        loaded.items[1].error.as_ref().unwrap().r#type,
        pdpb::ErrorType::GlobalConfigNotFound as i32
    );
    assert_eq!(
        loaded.items[1].error.as_ref().unwrap().message,
        "key missing not found"
    );
    assert_eq!(loaded.items[2].payload, vec![0, 255]);
    let mut evidence = vec![response.encode_to_vec(), loaded.encode_to_vec()];
    for path in [
        "OtherConfigPath",
        "/",
        "/global/configuration/",
        "resource_group/controllers",
        "resource_group/controller-other",
        "/pd/secret",
        "/global/config-sibling",
        "resource_group/controller/child",
    ] {
        let error = client
            .store_global_config(pdpb::StoreGlobalConfigRequest {
                config_path: path.into(),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        evidence.push(format!("{:?}:{}", error.code(), error.message()).into_bytes());
        let error = client
            .load_global_config(pdpb::LoadGlobalConfigRequest {
                config_path: path.into(),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        evidence.push(format!("{:?}:{}", error.code(), error.message()).into_bytes());
    }
    client
        .store_global_config(pdpb::StoreGlobalConfigRequest {
            config_path: "/global/config/sub-other".into(),
            changes: vec![pdpb::GlobalConfigItem {
                name: "sibling".into(),
                payload: b"sibling".to_vec(),
                ..Default::default()
            }],
        })
        .await
        .unwrap();
    for (path, count) in [("/global/config/sub", 3), ("/global/config/sub/", 2)] {
        let mut loaded = client
            .load_global_config(pdpb::LoadGlobalConfigRequest {
                config_path: path.into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(loaded.items.len(), count);
        assert!(loaded.revision > 0);
        loaded.revision = 0;
        evidence.push(loaded.encode_to_vec());
    }
    for path in ["", "/global/config", "/global/config/"] {
        let mut prefix = client
            .load_global_config(pdpb::LoadGlobalConfigRequest {
                config_path: path.into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner();
        assert!(prefix.revision > 0);
        assert!(
            prefix
                .items
                .iter()
                .any(|item| item.name == "/global/config/sub/binary")
        );
        prefix.revision = 0;
        evidence.push(prefix.encode_to_vec());
    }
    client
        .store_global_config(pdpb::StoreGlobalConfigRequest {
            config_path: "/global/config/sub".into(),
            changes: vec![pdpb::GlobalConfigItem {
                name: "binary".into(),
                kind: pdpb::EventType::Delete as i32,
                ..Default::default()
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    let removed = client
        .load_global_config(pdpb::LoadGlobalConfigRequest {
            config_path: "/global/config/sub".into(),
            names: vec!["binary".into(), "legacy".into()],
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(removed.items[0].error.is_some());
    assert_eq!(removed.items[1].payload, b"old");
    evidence.push(removed.encode_to_vec());
    // Names are joined literally, including empty names and slash/dot segments.
    for name in ["", "/absolute", "../literal", "percent%2f"] {
        client
            .store_global_config(pdpb::StoreGlobalConfigRequest {
                config_path: "/global/config/literal/".into(),
                changes: vec![pdpb::GlobalConfigItem {
                    name: name.into(),
                    payload: name.as_bytes().to_vec(),
                    ..Default::default()
                }],
            })
            .await
            .unwrap();
        let loaded = client
            .load_global_config(pdpb::LoadGlobalConfigRequest {
                config_path: "/global/config/literal/".into(),
                names: vec![name.into()],
            })
            .await
            .unwrap()
            .into_inner();
        assert!(loaded.items[0].error.is_none());
        assert_eq!(loaded.items[0].payload, name.as_bytes());
        evidence.push(loaded.encode_to_vec());
    }
    let error = client
        .load_global_config(pdpb::LoadGlobalConfigRequest {
            config_path: "resource_group/controller".into(),
            names: vec!["name".into()],
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    evidence.push(error.message().as_bytes().to_vec());
    // Oversized writes fail at the real backend boundary and commit neither key.
    let error = client
        .store_global_config(pdpb::StoreGlobalConfigRequest {
            changes: vec![
                pdpb::GlobalConfigItem {
                    name: "atomic-small".into(),
                    payload: b"not committed".to_vec(),
                    ..Default::default()
                },
                pdpb::GlobalConfigItem {
                    name: "atomic-large".into(),
                    payload: vec![1; 1600 * 1024],
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
        .await
        .unwrap_err();
    println!(
        "oversized backend status: {:?}: {}",
        error.code(),
        error.message()
    );
    assert_eq!(error.code(), tonic::Code::Unknown);
    assert_eq!(error.message(), "etcdserver: request is too large");
    evidence.push(format!("{:?}", error.code()).into_bytes());
    println!("oversized backend diagnostic: {}", error.message());
    let missing = client
        .load_global_config(pdpb::LoadGlobalConfigRequest {
            names: vec!["atomic-small".into(), "atomic-large".into()],
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(
        missing
            .items
            .iter()
            .all(|item| item.error.as_ref().unwrap().r#type
                == pdpb::ErrorType::GlobalConfigNotFound as i32)
    );
    evidence.push(missing.encode_to_vec());
    for path in [
        "/global/config/../pd/",
        "/global/config/child/../../pd/",
        "/global/config/./child/",
        "/global/config//child/",
        "/global/config//",
        r"/global/config/child\secret/",
    ] {
        client
            .store_global_config(pdpb::StoreGlobalConfigRequest {
                config_path: path.into(),
                changes: vec![pdpb::GlobalConfigItem {
                    name: "source_id".into(),
                    payload: path.as_bytes().to_vec(),
                    ..Default::default()
                }],
            })
            .await
            .unwrap();
        let loaded = client
            .load_global_config(pdpb::LoadGlobalConfigRequest {
                config_path: path.into(),
                names: vec!["source_id".into()],
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(loaded.items[0].payload, path.as_bytes());
        assert!(loaded.items[0].error.is_none());
        evidence.push(loaded.encode_to_vec());
    }
    let mut controller = client
        .load_global_config(pdpb::LoadGlobalConfigRequest {
            config_path: "resource_group/controller".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(controller.revision > 0);
    assert_eq!(controller.items.len(), 2);
    assert_eq!(controller.items[0].name, "resource_group/controller");
    assert!(!controller.items[0].payload.is_empty());
    assert_eq!(controller.items[1].name, "resource_group/controller-other");
    assert_eq!(controller.items[1].payload, b"sibling-value");
    controller.revision = 0;
    evidence.push(controller.encode_to_vec());
    evidence
}

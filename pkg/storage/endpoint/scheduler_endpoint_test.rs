// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, MemoryStore, MetaStore};
use std::sync::Arc;

async fn contract(store: Arc<dyn MetaStore>) {
    let ep = StorageEndpoint::new(store.clone());
    let paths = ClusterPaths::new(42);
    let base = Base::new(store.as_ref());
    assert!(
        ep.load_scheduler_config(&paths, "missing")
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        ep.load_all_scheduler_configs(&paths)
            .await
            .unwrap()
            .is_empty()
    );
    ep.remove_scheduler_config(&paths, "missing").await.unwrap();
    // Raw bytes: neither JSON parsing nor UTF-8 repair is permitted.
    let entries = [
        ("z", b"{not json".as_slice()),
        ("a", b"\xff\0raw"),
        ("", b""),
        ("nested/name", b"null"),
        ("/a//../b/", b"verbatim path"),
        ("调度", b"unicode"),
    ];
    for (name, data) in entries {
        ep.save_scheduler_config(&paths, name, data).await.unwrap();
        assert_eq!(ep.load_scheduler_config(&paths, name).await.unwrap(), data);
        assert_eq!(
            base.load(paths.scheduler_config_path(name).as_bytes())
                .await
                .unwrap(),
            data
        );
    }
    ep.save_scheduler_config(&paths, "a", b"overwritten")
        .await
        .unwrap();
    assert_eq!(
        ep.load_scheduler_config(&paths, "a").await.unwrap(),
        b"overwritten"
    );
    // Adjacent prefix keys and cluster/config keys must not enter the range.
    for key in [
        "/pd/42/scheduler_config",
        "/pd/42/scheduler_config0/a",
        "/pd/42/scheduler_config_extra/a",
        "/pd/42/config",
        "/pd/43/scheduler_config/a",
    ] {
        base.save(key.as_bytes(), b"outside").await.unwrap();
    }
    let mut expected: Vec<_> = entries
        .into_iter()
        .map(|(name, data)| {
            (
                name.as_bytes().to_vec(),
                if name == "a" {
                    b"overwritten".to_vec()
                } else {
                    data.to_vec()
                },
            )
        })
        .collect();
    let mut external_key = paths.scheduler_config_path_prefix().into_bytes();
    external_key.extend_from_slice(b"\xff");
    base.save(&external_key, b"external").await.unwrap();
    expected.push((vec![255], b"external".to_vec()));
    expected.sort();
    assert_eq!(
        ep.load_all_scheduler_configs(&paths).await.unwrap(),
        expected
    );
    ep.remove_scheduler_config(&paths, "a").await.unwrap();
    ep.remove_scheduler_config(&paths, "a").await.unwrap();
    assert!(
        ep.load_scheduler_config(&paths, "a")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        ep.load_scheduler_config(&paths, "nested/name")
            .await
            .unwrap(),
        b"null"
    );
    base.remove(&external_key).await.unwrap();
    for (name, _) in entries {
        ep.remove_scheduler_config(&paths, name).await.unwrap();
    }
    assert!(
        ep.load_all_scheduler_configs(&paths)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        base.load(b"/pd/43/scheduler_config/a").await.unwrap(),
        b"outside"
    );
    // Go LoadAllSchedulerConfigs is one range read with MinKVRangeLimit=100.
    for n in (0..=100).rev() {
        ep.save_scheduler_config(&paths, &format!("s{n:03}"), &[n as u8])
            .await
            .unwrap();
    }
    let rows = ep.load_all_scheduler_configs(&paths).await.unwrap();
    assert_eq!(rows.len(), 100);
    for (n, (name, data)) in rows.iter().enumerate() {
        assert_eq!(name, format!("s{n:03}").as_bytes());
        assert_eq!(data, &[n as u8]);
    }
    assert_eq!(
        ep.load_scheduler_config(&paths, "s100").await.unwrap(),
        [100]
    );
}
#[tokio::test]
async fn scheduler_endpoint_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn scheduler_endpoint_etcd() {
    let mut fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    let paths = ClusterPaths::new(99);
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    ep.save_scheduler_config(&paths, "persist", b"\xff\0")
        .await
        .unwrap();
    ep.save_scheduler_config(&paths, "empty", b"")
        .await
        .unwrap();
    ep.save_scheduler_config(&paths, "deleted", b"gone")
        .await
        .unwrap();
    ep.remove_scheduler_config(&paths, "deleted").await.unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let ep = StorageEndpoint::new(Arc::new(fixture.store().await));
    assert_eq!(
        ep.load_scheduler_config(&paths, "persist").await.unwrap(),
        b"\xff\0"
    );
    assert_eq!(
        ep.load_all_scheduler_configs(&paths).await.unwrap(),
        vec![
            (b"empty".to_vec(), vec![]),
            (b"persist".to_vec(), b"\xff\0".to_vec())
        ]
    );
    // Wait for actual server exit before exercising each IO failure.
    drop(fixture);
    assert!(matches!(
        ep.load_scheduler_config(&paths, "persist").await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.load_all_scheduler_configs(&paths).await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.save_scheduler_config(&paths, "persist", b"new").await,
        Err(EndpointError::Store(_))
    ));
    assert!(matches!(
        ep.remove_scheduler_config(&paths, "persist").await,
        Err(EndpointError::Store(_))
    ));
}

// Copyright 2026 AsterSQL.
use super::*;
use crate::grpc_storage::Storage;
use astersql_pd_keypath::{
    keyspace_resource_group_setting_path, keyspace_resource_group_state_path,
};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use std::sync::Arc;
#[path = "../../../storage/kv/etcd_fixture_test.rs"]
#[allow(dead_code)]
mod fixture;
fn group() -> Group {
    let mut g = Group::reserved_default();
    g.name = "work".into();
    g.ru.as_mut().unwrap().tokens = 50.0;
    g
}
async fn exists(store: &dyn MetaStore, key: String) -> bool {
    !store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap()
        .entries
        .is_empty()
}
#[tokio::test]
async fn mcs_094_write_roles_persist_only_owned_keys() {
    for role in [
        WriteRole::LegacyAll,
        WriteRole::PdMetaOnly,
        WriteRole::RmTokenOnly,
    ] {
        let fixture = fixture::Fixture::start();
        let store = Arc::new(fixture.store().await);
        let mut manager = Manager::load(Arc::new(Storage::new(store.clone())), role)
            .await
            .unwrap();
        if !role.metadata() {
            assert_eq!(
                manager.add(7, group()).await.unwrap_err(),
                "metadata write is disabled"
            );
            assert!(
                !exists(
                    store.as_ref(),
                    keyspace_resource_group_setting_path(7, "work")
                )
                .await
            );
            assert!(
                !exists(
                    store.as_ref(),
                    keyspace_resource_group_state_path(7, "work")
                )
                .await
            );
            continue;
        }
        manager.add(7, group()).await.unwrap();
        assert!(
            exists(
                store.as_ref(),
                keyspace_resource_group_setting_path(7, "work")
            )
            .await
        );
        assert_eq!(
            exists(
                store.as_ref(),
                keyspace_resource_group_state_path(7, "work")
            )
            .await,
            role.state()
        );
        manager.delete(7, "work").await.unwrap();
        assert!(
            !exists(
                store.as_ref(),
                keyspace_resource_group_setting_path(7, "work")
            )
            .await
        );
        assert!(
            !exists(
                store.as_ref(),
                keyspace_resource_group_state_path(7, "work")
            )
            .await
        );
    }
}
#[tokio::test]
async fn mcs_092_failed_modify_keeps_memory_patch_and_failed_delete_keeps_group() {
    let mut fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut manager = Manager::load(Arc::new(Storage::new(store.clone())), WriteRole::LegacyAll)
        .await
        .unwrap();
    manager.add(7, group()).await.unwrap();
    fixture.command("stop", "stopped");
    let mut change = group();
    change.priority = 16;
    change.ru.as_mut().unwrap().tokens = 12.0;
    assert!(manager.modify(7, change).await.is_err());
    let current = manager.get(7, "work", true).await.unwrap();
    assert_eq!(current.priority, 16);
    assert_eq!(current.ru.unwrap().tokens, 62.0);
    assert!(manager.delete(7, "work").await.is_err());
    assert_eq!(manager.get(7, "work", true).await.unwrap().priority, 16);
    fixture.command("start", "started");
    let persisted = Manager::load(Arc::new(Storage::new(store.clone())), WriteRole::LegacyAll)
        .await
        .unwrap();
    assert_eq!(persisted.groups[&7]["work"].priority, 8);
    assert_eq!(
        persisted.groups[&7]["work"].ru.as_ref().unwrap().tokens,
        50.0
    );
    manager.delete(7, "work").await.unwrap();
    assert!(manager.get(7, "work", true).await.is_err());
    assert!(
        !exists(
            store.as_ref(),
            keyspace_resource_group_setting_path(7, "work")
        )
        .await
    );
    assert!(
        !exists(
            store.as_ref(),
            keyspace_resource_group_state_path(7, "work")
        )
        .await
    );
}

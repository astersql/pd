// Copyright 2026 AsterSQL.
use crate::meta_group_assignment_oracle_test::http;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::KeyspaceGroupManager;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_domain::{
    Base, Consistency, KeyspaceGroup, MetaStore, Read, Snapshot, StorageEndpoint,
};
use serde_json::json;
use std::sync::Arc;

fn receipt(snapshot: &Snapshot) -> serde_json::Value {
    json!({"revision":snapshot.revision,"entries":snapshot.entries.iter().map(|e|json!({"key":String::from_utf8_lossy(&e.key),"value":String::from_utf8_lossy(&e.value),"create_revision":e.create_revision,"mod_revision":e.mod_revision,"lease":e.lease})).collect::<Vec<_>>()})
}

pub(crate) async fn start_api_go() -> (crate::harness::Server, u64) {
    start_bootstrapped_go(Some("api")).await
}

pub(crate) async fn start_bootstrapped_go(mode: Option<&str>) -> (crate::harness::Server, u64) {
    let binary = crate::readonly_oracle::pinned_go_binary().unwrap();
    let environment = mode
        .map(|mode| vec![("PD_SERVICE_MODE".into(), mode.into())])
        .unwrap_or_default();
    let mut go =
        crate::harness::Server::start_with_environment(&binary, &[], &environment).unwrap();
    go.ready().unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let bootstrap = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:20160".into(),
                ..Default::default()
            }),
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
    assert!(bootstrap.header.unwrap().error.is_none());
    (go, id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the admitted read-only Go PD binary and official etcd"]
async fn mcs_032_default_group_go_differential() {
    let (go, id) = start_api_go().await;
    let fixture = crate::harness::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let manager = KeyspaceGroupManager::new(store.clone(), ClusterPaths::new(42));
    manager.bootstrap().await.unwrap();
    let groups: Vec<_> = [(1, vec![111, 222]), (2, vec![222, 333])]
        .into_iter()
        .map(|(id, keys)| KeyspaceGroup {
            id,
            user_kind: "standard".into(),
            keyspaces: Some(keys),
            ..Default::default()
        })
        .collect();
    let url = format!("{}/pd/api/v2/tso/keyspace-groups", go.url);
    // Host-provided member addresses become explicit shared fixture inputs.
    let initial = http(&format!("{url}/0"), "GET", json!(null));
    assert_eq!(initial.0, 200, "{initial:?}");
    let initial: KeyspaceGroup = serde_json::from_value(initial.1).unwrap();
    let mut txn = Base::new(store.as_ref()).begin_txn();
    StorageEndpoint::new(store.clone())
        .save_keyspace_group(&mut txn, &ClusterPaths::new(42), &initial)
        .unwrap();
    txn.commit::<String>().await.unwrap();

    assert_eq!(http(&url, "POST", json!({"keyspace-groups":groups})).0, 200);
    manager.create_groups(&groups).await.unwrap();
    let go_store = astersql_pd_storage_kv::EtcdMetaStore::connect(
        &[go.url.clone()],
        std::time::Duration::from_secs(2),
    )
    .await
    .unwrap();
    let go_paths = ClusterPaths::new(id);
    let go_before = go_store
        .read(Read::prefix(
            go_paths.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let rust_before = store
        .read(Read::prefix(
            ClusterPaths::new(42).keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(go_before.entries.len(), 3);
    assert_eq!(rust_before.entries.len(), 3);

    let merged_response = http(
        &format!("{url}/0/merge"),
        "POST",
        json!({"merge-list":[1,2]}),
    );
    assert_eq!(merged_response.0, 200, "{merged_response:?}");
    manager.merge_groups(0, &[1, 2]).await.unwrap();
    let go_merged = http(&format!("{url}/0"), "GET", json!(null));
    assert_eq!(go_merged.0, 200);
    let actual: KeyspaceGroup = serde_json::from_value(go_merged.1.clone()).unwrap();
    let rust = manager.get_group(0).await.unwrap().unwrap();
    assert_eq!(actual, rust);
    let go_after = go_store
        .read(Read::prefix(
            go_paths.keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let rust_after = store
        .read(Read::prefix(
            ClusterPaths::new(42).keyspace_group_id_prefix().as_bytes(),
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(go_after.entries.len(), 1);
    assert_eq!(rust_after.entries.len(), 1);
    assert_eq!(go_after.entries[0].value, rust_after.entries[0].value);
    for (before, after) in [(&go_before, &go_after), (&rust_before, &rust_after)] {
        assert_eq!(
            before.entries[0].create_revision,
            after.entries[0].create_revision
        );
        assert!(after.entries[0].mod_revision > before.entries[0].mod_revision);
    }

    for id in [1, 2] {
        assert!(http(&format!("{url}/{id}"), "GET", json!(null)).1.is_null());
        assert!(manager.get_group(id).await.unwrap().is_none());
    }
    let finish = http(&format!("{url}/0/merge"), "DELETE", json!(null));
    assert_eq!(finish.0, 200, "{finish:?}");
    manager.finish_merge(0).await.unwrap();
    let go_finished = http(&format!("{url}/0"), "GET", json!(null));
    let actual: KeyspaceGroup = serde_json::from_value(go_finished.1.clone()).unwrap();
    let rust = manager.get_group(0).await.unwrap().unwrap();
    assert_eq!(actual, rust);
    if let Some(path) = std::env::var_os("MCS_032_ORACLE_REPORT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"go_merged":go_merged,"go_finished":go_finished,"rust_finished":rust,"physical_go_before":receipt(&go_before),"physical_go_after":receipt(&go_after),"physical_rust_before":receipt(&rust_before),"physical_rust_after":receipt(&rust_after)}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}

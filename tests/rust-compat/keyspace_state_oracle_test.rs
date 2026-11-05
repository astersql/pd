// Copyright 2026 AsterSQL.
use crate::meta_group_assignment_oracle_test::http;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_keyspace::{KeyspaceCreateResources, KeyspaceManager, MetaServiceGroupManager};
use astersql_pd_proto::{
    keyspacepb::{self, keyspace_client::KeyspaceClient},
    pdpb,
};
use astersql_pd_storage_domain::{
    Base, Consistency, Keyspace, KeyspaceCodec, KeyspaceIdentity, MetaServiceGroupStatus,
    MetaStore, Read, StorageEndpoint,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};
fn from_wire(meta: keyspacepb::KeyspaceMeta) -> Keyspace {
    Keyspace {
        identity: meta.keyspace.map(|id| match id {
            keyspacepb::keyspace_meta::Keyspace::Id(id) => KeyspaceIdentity::Id(id),
            _ => panic!("fixture expects classic ID"),
        }),
        name: meta.name,
        state: meta.state,
        created_at: meta.created_at,
        state_changed_at: meta.state_changed_at,
        config: meta.config,
    }
}
async fn physical(store: &dyn MetaStore, p: &ClusterPaths, id: u32) -> Value {
    let key = p.keyspace_meta_path(id);
    let s = store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap();
    json!({"key":key,"revision":s.revision,"entries":s.entries.iter().map(|e|json!({"value_len":e.value.len(),"value_sha256":format!("{:x}",Sha256::digest(&e.value)),"value_hex":if e.value.len()<8192{Some(e.value.iter().map(|b|format!("{b:02x}")).collect::<String>())}else{None},"create_revision":e.create_revision,"mod_revision":e.mod_revision})).collect::<Vec<_>>()})
}
async fn seed(store: Arc<dyn MetaStore>, paths: &ClusterPaths, meta: &Keyspace) {
    let adapter = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone());
    let storage = adapter.keyspace_storage(paths.clone());
    let mut txn = Base::new(store.as_ref()).begin_txn();
    storage.save_keyspace_meta(&mut txn, paths, meta).unwrap();
    storage
        .save_keyspace_id(&mut txn, paths, meta.id(), &meta.name)
        .unwrap();
    txn.commit::<String>().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_039_state_go_rpc_http_differential() {
    let (mut go, cluster) = crate::group_merge_oracle_test::start_bootstrapped_go(None).await;
    let gs: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let gp = ClusterPaths::new(cluster);
    let mut fixture = crate::harness::fixture::Fixture::start();
    let rs: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let rp = ClusterPaths::new(42);
    let m = KeyspaceManager::with_resources(
        astersql_pd_storage_endpoint::StorageEndpoint::new(rs.clone()).keyspace_storage(rp.clone()),
        KeyspaceCreateResources {
            meta_groups: Some(Arc::new(MetaServiceGroupManager::new(
                rs.clone(),
                rp.clone(),
                BTreeMap::new(),
            ))),
            ..Default::default()
        },
    );
    let mut client = KeyspaceClient::connect(go.url.clone()).await.unwrap();
    let mut receipts = vec![];
    for (i, from) in [0, 1, 2, 3, 99].into_iter().enumerate() {
        for (j, to) in [0, 1, 2, 3, 99].into_iter().enumerate() {
            let id = 100 + (i * 5 + j) as u32;
            let initial = Keyspace {
                identity: Some(KeyspaceIdentity::Id(id)),
                name: format!("state{id}"),
                state: from,
                created_at: 100,
                state_changed_at: 100,
                ..Default::default()
            };
            seed(gs.clone(), &gp, &initial).await;
            m.save_new(&initial).await.unwrap();
            let before_go = physical(gs.as_ref(), &gp, id).await;
            let before_rust = physical(rs.as_ref(), &rp, id).await;
            let response = client
                .update_keyspace_state(keyspacepb::UpdateKeyspaceStateRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: cluster,
                        ..Default::default()
                    }),
                    state: to,
                    keyspace: Some(keyspacepb::update_keyspace_state_request::Keyspace::Id(id)),
                })
                .await
                .unwrap()
                .into_inner();
            let header = response.header.unwrap();
            let now = response
                .keyspace
                .as_ref()
                .map_or(999, |m| m.state_changed_at);
            let rust = m.update_state_by_id(id, to, now).await;
            let outcome = match (header.error, response.keyspace, rust) {
                (None, Some(actual), Ok(expected)) => {
                    assert_eq!(from_wire(actual), expected);
                    json!({"state":expected.state,"state_changed_at":expected.state_changed_at})
                }
                (Some(error), None, Err(rust)) => {
                    assert_eq!(error.message, rust.to_string());
                    json!({"error_type":error.r#type,"message":error.message})
                }
                other => panic!("unexpected state results: {other:?}"),
            };
            let after_go = physical(gs.as_ref(), &gp, id).await;
            let after_rust = physical(rs.as_ref(), &rp, id).await;
            if outcome.get("message").is_some() {
                assert_eq!(before_go["entries"], after_go["entries"]);
                assert_eq!(before_rust["entries"], after_rust["entries"]);
            }
            assert_eq!(
                after_go["entries"][0]["value_sha256"],
                after_rust["entries"][0]["value_sha256"]
            );
            receipts.push(json!({"case":"rpc_matrix","id":id,"from":from,"to":to,"outcome":outcome,"cluster_id":header.cluster_id,"before_go":before_go,"after_go":after_go,"before_rust":before_rust,"after_rust":after_rust}));
        }
    }
    for id in [0, 50000] {
        let response = client
            .update_keyspace_state(keyspacepb::UpdateKeyspaceStateRequest {
                header: Some(pdpb::RequestHeader {
                    cluster_id: cluster,
                    ..Default::default()
                }),
                state: 1,
                keyspace: Some(keyspacepb::update_keyspace_state_request::Keyspace::Id(id)),
            })
            .await
            .unwrap()
            .into_inner();
        let error = response.header.unwrap().error.unwrap();
        let rust = m.update_state_by_id(id, 1, 999).await.unwrap_err();
        assert_eq!(error.message, rust.to_string());
        receipts.push(json!({"case":"rpc_protected_or_missing","id":id,"error_type":error.r#type,"message":error.message}));
    }
    let initial = Keyspace {
        identity: Some(KeyspaceIdentity::Id(200)),
        name: "named_state".into(),
        state: 0,
        created_at: 100,
        state_changed_at: 100,
        ..Default::default()
    };
    seed(gs.clone(), &gp, &initial).await;
    m.save_new(&initial).await.unwrap();
    for state in [1, 1, 0, 1, 2, 3, 0] {
        let state_name = match state {
            0 => "ENABLED",
            1 => "DISABLED",
            2 => "ARCHIVED",
            _ => "TOMBSTONE",
        };
        let response = http(
            &format!("{}/pd/api/v2/keyspaces/named_state/state", go.url),
            "PUT",
            json!({"state":state_name}),
        );
        let now = response.1["state_changed_at"].as_i64().unwrap_or(999);
        let rust = m.update_state_by_name("named_state", state, now).await;
        match rust {
            Ok(meta) => assert_eq!(
                response,
                (200, crate::keyspace_create_oracle_test::dto(&meta))
            ),
            Err(error) => assert_eq!(response, (500, json!(error.to_string()))),
        }
        receipts.push(json!({"case":"http_name","state":state,"response":response}));
    }
    let protected = http(
        &format!("{}/pd/api/v2/keyspaces/DEFAULT/state", go.url),
        "PUT",
        json!({"state":"DISABLED"}),
    );
    assert_eq!(
        protected,
        (
            500,
            json!(
                m.update_state_by_name("DEFAULT", 1, 999)
                    .await
                    .unwrap_err()
                    .to_string()
            )
        )
    );
    // Repair a legacy tombstone binding through actual persisted metadata/counter inputs.
    let mut legacy = m.load_by_id(200).await.unwrap();
    legacy
        .config
        .insert("meta_service_group_id".into(), "removed_group".into());
    for (store, paths) in [(&gs, &gp), (&rs, &rp)] {
        seed(store.clone(), paths, &legacy).await;
        let mut txn = Base::new(store.as_ref()).begin_txn();
        StorageEndpoint::new(store.clone())
            .save_meta_service_group_status(
                &mut txn,
                paths,
                "removed_group",
                Some(&MetaServiceGroupStatus {
                    enabled: true,
                    assignment_count: 1,
                }),
            )
            .unwrap();
        txn.commit::<String>().await.unwrap();
    }
    let response = http(
        &format!("{}/pd/api/v2/keyspaces/named_state/state", go.url),
        "PUT",
        json!({"state":"TOMBSTONE"}),
    );
    let repaired = m.update_state_by_id(200, 3, 999).await.unwrap();
    assert_eq!(
        response,
        (200, crate::keyspace_create_oracle_test::dto(&repaired))
    );
    for (store, paths) in [(&gs, &gp), (&rs, &rp)] {
        let mut txn = Base::new(store.as_ref()).begin_txn();
        let status = StorageEndpoint::new(store.clone())
            .load_meta_service_group_status(
                &mut txn,
                paths,
                &BTreeMap::from([("removed_group".into(), String::new())]),
            )
            .await
            .unwrap();
        assert_eq!(status["removed_group"].assignment_count, 0);
    }
    receipts.push(json!({"case":"legacy_tombstone_repair","http":response,"go":physical(gs.as_ref(),&gp,200).await,"rust":physical(rs.as_ref(),&rp,200).await}));
    // Seed a real readable metadata value whose subsequent Go transaction exceeds its SDK send limit.
    let codec = astersql_pd_storage_endpoint::ProtobufKeyspaceCodec;
    let mut huge = Keyspace {
        identity: Some(KeyspaceIdentity::Id(300)),
        name: "state_fault".into(),
        state: 1,
        created_at: 100,
        state_changed_at: 100,
        config: HashMap::from([("filler".into(), "x".repeat(2 * 1024 * 1024 - 256))]),
    };
    let delta = 2 * 1024 * 1024 - codec.encode(&huge).len();
    huge.config
        .get_mut("filler")
        .unwrap()
        .push_str(&"x".repeat(delta));
    assert_eq!(codec.encode(&huge).len(), 2 * 1024 * 1024);
    seed(gs.clone(), &gp, &huge).await;
    let before = physical(gs.as_ref(), &gp, 300).await;
    let failure = http(
        &format!("{}/pd/api/v2/keyspaces/state_fault/state", go.url),
        "PUT",
        json!({"state":"ENABLED"}),
    );
    assert_eq!(failure.0, 500, "{failure:?}");
    assert!(failure.1.as_str().unwrap().contains("larger than max"));
    let after = physical(gs.as_ref(), &gp, 300).await;
    assert_eq!(before["entries"], after["entries"]);
    receipts.push(json!({"case":"go_state_commit_rejection","input":{"initial_state":1,"encoded_meta_bytes":2*1024*1024,"target_state":0},"http":failure,"before":before,"after":after}));
    go.stop().unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let recovered_rust = physical(&fixture.store().await, &rp, 200).await;
    go.restart_with_args(&crate::readonly_oracle::pinned_go_binary().unwrap(), &[])
        .unwrap();
    go.ready().unwrap();
    let recovered_go_store =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let recovered_go = physical(&recovered_go_store, &gp, 200).await;
    assert_eq!(
        recovered_go["entries"][0]["value_sha256"],
        recovered_rust["entries"][0]["value_sha256"]
    );
    assert_eq!(
        physical(&recovered_go_store, &gp, 300).await["entries"],
        after["entries"]
    );
    receipts.push(json!({"case":"same_wal","go":recovered_go,"rust":recovered_rust}));
    if let Some(path) = std::env::var_os("MCS_039_ORACLE_REPORT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({"cluster_id":cluster,"cases":receipts})).unwrap(),
        )
        .unwrap();
    }
}

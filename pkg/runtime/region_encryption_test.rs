// Copyright 2026 AsterSQL.
use crate::node_test::fixture;
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{
    encryptionpb, metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use prost::Message;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_015_runtime_heartbeat_encrypts_and_restart_decrypts() {
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join(format!("mcs015-runtime-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _directory = Directory(root.clone());
    let file = root.join("master");
    std::fs::write(
        &file,
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    )
    .unwrap();
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let mut settings = astersql_pd_server_config::Config::default();
    settings.security.encryption.data_encryption_method = "aes256-ctr".into();
    settings.security.encryption.master_key.key_type = "file".into();
    settings.security.encryption.master_key.file.file_path = file.to_str().unwrap().into();
    settings.security.encryption.adjust().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}");
    let identity = NodeConfig {
        cluster_id: 1510,
        member_id: 1,
        name: "encrypted".into(),
        client_url: url.clone(),
        peer_url: "http://127.0.0.1:2380".into(),
    };
    let parent = CancellationToken::new();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent)
        .with_listener_config(settings.clone())
        .with_telemetry_config(Default::default())
        .start(
            store.clone(),
            identity.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let keys = store
        .read(Read::range(
            b"encryption_keys",
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(keys.entries.len(), 1);
    let encrypted =
        encryptionpb::EncryptedContent::decode(keys.entries[0].value.as_slice()).unwrap();
    assert_eq!(encrypted.iv.len(), 12);
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: 1510,
        ..Default::default()
    });
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    let boot = client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "store-1:20160".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 100,
                peers: vec![peer.clone()],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(boot.header.unwrap().error.is_none());
    let paths = ClusterPaths::new(1510);
    let bootstrap = store
        .read(Read::range(
            paths.region_path(100).as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let bootstrap = metapb::Region::decode(bootstrap.entries[0].value.as_slice()).unwrap();
    assert!(
        bootstrap.encryption_meta.is_some(),
        "bootstrap secondary save must use the active encryption dictionary"
    );
    let original = metapb::Region {
        id: 200,
        start_key: b"app/a-long-unaligned-bound".to_vec(),
        end_key: b"app/zz".to_vec(),
        region_epoch: Some(metapb::RegionEpoch {
            version: 2,
            conf_ver: 1,
        }),
        peers: vec![peer.clone()],
        is_in_flashback: true,
        flashback_start_ts: 12345,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let mut replies = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    tx.send(pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(original.clone()),
        leader: Some(peer),
        approximate_size: 2 * 1048576,
        ..Default::default()
    })
    .await
    .unwrap();
    let persisted=tokio::time::timeout(Duration::from_secs(6),async {loop {
        let rows=store.read(Read::range(paths.region_path(200).as_bytes(),b"",Consistency::Linearizable)).await.unwrap();if let Some(row)=rows.entries.first(){break metapb::Region::decode(row.value.as_slice()).unwrap();}
        tokio::select!{response=replies.message()=>{if let Some(response)=response.unwrap(){assert!(response.header.unwrap().error.is_none());}},_=tokio::time::sleep(Duration::from_millis(10))=>{}}
    }}).await.unwrap();
    assert!(persisted.encryption_meta.is_some());
    assert_ne!(persisted.start_key, original.start_key);
    assert_ne!(persisted.end_key, original.end_key);
    let mut fields = persisted.clone();
    fields.start_key = original.start_key.clone();
    fields.end_key = original.end_key.clone();
    fields.encryption_meta = None;
    assert_eq!(fields, original);
    let response = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header: header.clone(),
            region_id: 200,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.region.as_ref(), Some(&original));
    drop((tx, replies, client));
    node.shutdown().await.unwrap();
    assert!(node.try_cluster().is_none());
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let mut restarted =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .with_listener_config(settings)
            .start(
                store.clone(),
                identity,
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    restarted.ready().await.unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    let response = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header,
            region_id: 200,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.region.as_ref(), Some(&original));
    let still_encrypted = store
        .read(Read::range(
            paths.region_path(200).as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert!(
        metapb::Region::decode(still_encrypted.entries[0].value.as_slice())
            .unwrap()
            .encryption_meta
            .is_some()
    );
    restarted.shutdown().await.unwrap();
    println!(
        "Rust live heartbeat stored encrypted bounds and recovered exact original metadata over a real node restart"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_015_go_region_cipher_and_rust_cipher_restore_interoperate() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    use astersql_pd_core::{KeyRange, RegionEncryptionMeta, RegionMeta};
    use astersql_pd_encryption::{DictionaryStorage, decrypt_region, encrypt_region};
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join(format!("mcs015-go-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _directory = Directory(root.clone());
    let file = root.join("master");
    std::fs::write(
        &file,
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    )
    .unwrap();
    let toml = root.join("pd.toml");
    std::fs::write(&toml,format!("[security.encryption]\ndata-encryption-method='aes256-ctr'\n[security.encryption.master-key]\ntype='file'\npath={}\n[pd-server]\nuse-region-storage=true\n[schedule]\nhalt-scheduling=true\n[log]\nformat='json'\n",serde_json::to_string(file.to_str().unwrap()).unwrap())).unwrap();
    let binary = pinned_go_binary().unwrap();
    let mut go =
        Server::start_with_args(&binary, &[&format!("--config={}", toml.display())]).unwrap();
    go.ready_with_timeout(Duration::from_secs(12)).unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let cluster = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    client
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "store-1:20160".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 100,
                peers: vec![peer.clone()],
                ..Default::default()
            }),
        })
        .await
        .unwrap();
    let store: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let manager = DictionaryStorage::new(
        store.clone(),
        Arc::new(astersql_pd_server::encryption_dictionary::ProtoKeyDictionaryCodec),
    );
    manager.load().await.unwrap();
    assert_eq!(manager.get_current_key().unwrap().unwrap().1.key.len(), 32);
    let original = metapb::Region {
        id: 200,
        start_key: b"app/aa-unaligned-boundary".to_vec(),
        end_key: b"app/zz".to_vec(),
        region_epoch: Some(metapb::RegionEpoch {
            version: 2,
            conf_ver: 1,
        }),
        peers: vec![peer.clone()],
        is_in_flashback: true,
        flashback_start_ts: 12345,
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let mut sender = client.clone();
    let heartbeat = tokio_util::task::AbortOnDropHandle::new(tokio::spawn(async move {
        let mut responses = sender
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner();
        while let Some(response) = responses.message().await.unwrap() {
            assert!(response.header.unwrap().error.is_none());
        }
    }));
    tx.send(pdpb::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(original.clone()),
        leader: Some(peer),
        approximate_size: 2 * 1048576,
        ..Default::default()
    })
    .await
    .unwrap();
    let paths = ClusterPaths::new(cluster);
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let response = client
                .get_region_by_id(pdpb::GetRegionByIdRequest {
                    header: header.clone(),
                    region_id: 200,
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            if response.region.as_ref() == Some(&original) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(tx);
    if !heartbeat.is_finished() {
        heartbeat.abort();
    }
    if let Err(error) = heartbeat.await {
        assert!(error.is_cancelled(), "heartbeat task failed: {error}");
    }
    drop(client);
    go.stop().unwrap();
    let database_path = go.dir.join("data/region-meta");
    let mut database = rusty_leveldb::DB::open(
        &database_path,
        rusty_leveldb::Options {
            create_if_missing: false,
            paranoid_checks: true,
            ..Default::default()
        },
    )
    .unwrap();
    let persisted = metapb::Region::decode(
        database
            .get(paths.region_path(200).as_bytes())
            .unwrap()
            .as_ref(),
    )
    .unwrap();
    let cipher = persisted
        .encryption_meta
        .as_ref()
        .expect("Go's actual LevelDB endpoint must store encrypted bounds");
    let mut bounds = RegionMeta {
        id: persisted.id,
        key_range: KeyRange {
            start_key: persisted.start_key.clone(),
            end_key: persisted.end_key.clone(),
        },
        encryption_meta: Some(RegionEncryptionMeta {
            key_id: cipher.key_id,
            iv: cipher.iv.clone(),
        }),
        ..Default::default()
    };
    decrypt_region(Some(&mut bounds), Some(&manager))
        .await
        .unwrap();
    let mut plain = persisted.clone();
    plain.start_key = bounds.key_range.start_key;
    plain.end_key = bounds.key_range.end_key;
    plain.encryption_meta = None;
    assert_eq!(plain, original);
    let input = RegionMeta {
        id: original.id,
        key_range: KeyRange {
            start_key: original.start_key.clone(),
            end_key: original.end_key.clone(),
        },
        ..Default::default()
    };
    let encrypted = encrypt_region(Some(&input), Some(&manager))
        .unwrap()
        .into_owned();
    let mut from_rust = original.clone();
    from_rust.start_key = encrypted.key_range.start_key;
    from_rust.end_key = encrypted.key_range.end_key;
    let metadata = encrypted.encryption_meta.unwrap();
    from_rust.encryption_meta = Some(encryptionpb::EncryptionMeta {
        key_id: metadata.key_id,
        iv: metadata.iv,
    });
    database
        .put(
            paths.region_path(200).as_bytes(),
            &from_rust.encode_to_vec(),
        )
        .unwrap();
    database.flush().unwrap();
    database.close().unwrap();
    drop(database);
    go.restart_with_args(&binary, &[&format!("--config={}", toml.display())])
        .unwrap();
    go.ready_with_timeout(Duration::from_secs(12)).unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let response = client
        .get_region_by_id(pdpb::GetRegionByIdRequest {
            header,
            region_id: 200,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.region.as_ref(), Some(&original));
    println!(
        "Rust decrypted actual Go LevelDB CTR bounds; Go restart loaded Rust CTR bounds with the same current key ID and all non-key fields unchanged"
    );
    go.stop().unwrap();
}

#[tokio::test]
async fn mcs_015_history_storage_cipher_preserves_plaintext_api_and_old_keys() {
    use astersql_pd_encryption::{
        DataKey, DictionaryStorage, EncryptionMethod, KeyDictionary, MasterKeyMeta,
    };
    use astersql_pd_member::Leadership;
    use astersql_pd_storage::{HistoryHotRegion, HistoryQuery, HotHistory};
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join(format!("mcs015-history-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _directory = Directory(root.clone());
    let file = root.join("master");
    std::fs::write(
        &file,
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
    )
    .unwrap();
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1511,
        b"history-owner".to_vec(),
        60,
    )
    .await
    .unwrap();
    let dictionary = Arc::new(DictionaryStorage::new(
        store.clone(),
        Arc::new(astersql_pd_server::encryption_dictionary::ProtoKeyDictionaryCodec),
    ));
    let mut keys = KeyDictionary {
        current_key_id: 7,
        keys: std::collections::BTreeMap::from([(
            7,
            DataKey {
                key: vec![7; 24],
                method: EncryptionMethod::Aes192Ctr,
                creation_time: 7,
                was_exposed: false,
            },
        )]),
    };
    let meta = MasterKeyMeta::File {
        path: file.to_str().unwrap().into(),
    };
    dictionary.save(&term, &meta, keys.clone()).await.unwrap();
    dictionary.load().await.unwrap();
    let history = HotHistory::new(store.clone(), 1511, 1).with_cipher(
        crate::hot_history_encryption::codec(Some(dictionary.clone())),
    );
    let original = HistoryHotRegion {
        update_time: 100,
        region_id: 200,
        peer_id: 2,
        store_id: 1,
        is_leader: true,
        hot_region_type: "read".into(),
        hot_degree: 5,
        flow_bytes: 3.5,
        flow_cpu: 1.5,
        key_rate: 2.5,
        query_rate: 4.5,
        start_key: "00AABBCCDDEEFF".into(),
        end_key: "FF00AABBCCDDEE".into(),
        ..Default::default()
    };
    history
        .append(&[original.clone()], &CancellationToken::new())
        .await
        .unwrap();
    let raw = store
        .read(Read::prefix(
            b"/pd/1511/rust/hot-history/1/rows/",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(raw.entries.len(), 1);
    let stored: HistoryHotRegion = serde_json::from_slice(&raw.entries[0].value).unwrap();
    assert_ne!(stored.start_key, original.start_key);
    assert_ne!(stored.end_key, original.end_key);
    assert_eq!(stored.encryption_meta.as_ref().unwrap()["key_id"], 7);
    assert!(original.encryption_meta.is_none());
    let query = HistoryQuery {
        start_time: 1,
        end_time: 1000,
        is_leaders: vec![true],
        is_learners: vec![false],
        ..Default::default()
    };
    assert_eq!(
        history
            .query(&query, &CancellationToken::new())
            .await
            .unwrap(),
        Some(vec![original.clone()])
    );
    keys.current_key_id = 8;
    keys.keys.insert(
        8,
        DataKey {
            key: vec![8; 32],
            method: EncryptionMethod::Aes256Ctr,
            creation_time: 8,
            was_exposed: false,
        },
    );
    dictionary.save(&term, &meta, keys).await.unwrap();
    dictionary.load().await.unwrap();
    assert_eq!(
        history
            .query(&query, &CancellationToken::new())
            .await
            .unwrap(),
        Some(vec![original])
    );
    assert!(
        HotHistory::new(store, 1511, 1)
            .query(&query, &CancellationToken::new())
            .await
            .is_err()
    );
    term.resign().await.unwrap();
    println!(
        "Existing Rust history namespace stores lossless CTR ciphertext; API rows and historical-key reads stay plaintext and byte-exact"
    );
}

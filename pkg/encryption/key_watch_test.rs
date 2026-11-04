// Copyright 2026 AsterSQL.
use crate::etcd_fixture as fixture;
use crate::protocol_adapter::ProtoKeyDictionaryCodec;
use crate::{
    DataKey, DictionaryStorage, EncryptionConfig, EncryptionMethod, KeyDictionary, KeyRotation,
    KeyWatch, MasterKeyConfig, MasterKeyFileConfig,
};
use astersql_pd_election::Leadership;
use astersql_pd_storage_domain::MetaStore;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
fn keys(id: u64) -> KeyDictionary {
    KeyDictionary {
        current_key_id: id,
        keys: std::collections::BTreeMap::from([(
            id,
            DataKey {
                key: vec![1; 16],
                method: EncryptionMethod::Aes128Ctr,
                creation_time: 1601679533,
                was_exposed: false,
            },
        )]),
    }
}
fn policy(path: &str) -> EncryptionConfig {
    let mut config = EncryptionConfig {
        data_encryption_method: "aes128-ctr".into(),
        master_key: MasterKeyConfig {
            key_type: "file".into(),
            file: MasterKeyFileConfig {
                file_path: path.into(),
            },
            ..Default::default()
        },
        ..Default::default()
    };
    config.adjust().unwrap();
    config
}
async fn await_revision(storage: &DictionaryStorage, revision: i64) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if storage.snapshot().revision == revision {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("watch must converge to actual committed key revision");
}
#[tokio::test]
async fn mcs_014_behavior() {
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let config = policy(file.to_str().unwrap());
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1400,
        b"key-owner".to_vec(),
        30,
    )
    .await
    .unwrap();
    let writer = DictionaryStorage::new(store.clone(), Arc::new(ProtoKeyDictionaryCodec));
    writer
        .save(&term, &config.master_key_meta().unwrap(), keys(100))
        .await
        .unwrap();
    let reader = Arc::new(DictionaryStorage::new(
        store,
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    let rotation = Arc::new(KeyRotation::new(reader.clone(), &config).unwrap());
    let cancel = CancellationToken::new();
    let watch = KeyWatch::start(&tokio::runtime::Handle::current(), rotation, &cancel)
        .await
        .unwrap();
    let before = reader.snapshot();
    let receipt = writer
        .save(&term, &config.master_key_meta().unwrap(), keys(200))
        .await
        .unwrap();
    await_revision(&reader, receipt.revision).await;
    assert_eq!(reader.snapshot().keys.as_ref().unwrap().current_key_id, 200);
    assert_eq!(before.keys.as_ref().unwrap().current_key_id, 100);
    watch.shutdown().await.unwrap();
    let frozen = reader.snapshot();
    writer
        .save(&term, &config.master_key_meta().unwrap(), keys(300))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(reader.snapshot().revision, frozen.revision);
    let restarted = KeyWatch::start(
        &tokio::runtime::Handle::current(),
        Arc::new(KeyRotation::new(reader.clone(), &config).unwrap()),
        &cancel,
    )
    .await
    .unwrap();
    await_native_watchers(&fixture.endpoint, 1).await;
    drop(restarted);
    await_native_watchers(&fixture.endpoint, 0).await;
    term.resign().await.unwrap();
}

async fn await_failure(watch: &KeyWatch, phase: crate::KeyWatchPhase) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if watch
                .last_failure()
                .is_some_and(|failure| failure.phase == phase)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("actual watch error must reach diagnostics");
}
async fn await_native_watchers(endpoint: &str, count: u64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let output = std::process::Command::new("curl")
                .args([
                    "--silent",
                    "--max-time",
                    "2",
                    &format!("{endpoint}/metrics"),
                ])
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&output.stdout);
            if text
                .lines()
                .any(|s| s == format!("etcd_debugging_mvcc_watcher_total {count}"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("native etcd watcher resources must converge");
}
#[tokio::test]
async fn mcs_014_compaction_corruption_delete_and_exact_key() {
    use astersql_pd_storage_domain::{Consistency, Mutation, Read};
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let config = policy(file.to_str().unwrap());
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1401,
        b"key-owner".to_vec(),
        60,
    )
    .await
    .unwrap();
    let writer = DictionaryStorage::new(store.clone(), Arc::new(ProtoKeyDictionaryCodec));
    let initial = writer
        .save(&term, &config.master_key_meta().unwrap(), keys(100))
        .await
        .unwrap();
    let mut header = initial.revision;
    for i in 0..12 {
        header = store
            .transact(
                vec![],
                vec![Mutation::put(format!("unrelated/{i}").as_bytes(), b"data")],
            )
            .await
            .unwrap()
            .revision;
    }
    store.compact(header).await.unwrap();
    let reader = Arc::new(DictionaryStorage::new(
        store.clone(),
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    let rotation = Arc::new(KeyRotation::new(reader.clone(), &config).unwrap());
    let watch = KeyWatch::start(
        &tokio::runtime::Handle::current(),
        rotation,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    await_failure(&watch, crate::KeyWatchPhase::Compaction).await;
    assert_eq!(reader.snapshot().revision, initial.revision);
    let receipt = writer
        .save(&term, &config.master_key_meta().unwrap(), keys(200))
        .await
        .unwrap();
    await_revision(&reader, receipt.revision).await;
    let valid = reader.snapshot();
    store
        .transact(
            vec![],
            vec![Mutation::put(crate::ENCRYPTION_KEYS_PATH, &[0xff])],
        )
        .await
        .unwrap();
    await_failure(&watch, crate::KeyWatchPhase::Decode).await;
    let failure = watch.last_failure().unwrap();
    assert!(failure.message.contains("ErrProtoUnmarshal"));
    assert!(Arc::ptr_eq(
        reader.snapshot().keys.as_ref().unwrap(),
        valid.keys.as_ref().unwrap()
    ));
    store
        .transact(
            vec![],
            vec![Mutation::put(b"encryption_keys_backup", &[0xff])],
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(watch.last_failure().unwrap().message, failure.message);
    assert_eq!(reader.snapshot().revision, valid.revision);
    store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: crate::ENCRYPTION_KEYS_PATH.to_vec(),
            }],
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if watch
                .last_failure()
                .is_some_and(|f| f.message.contains("deleted unexpectedly"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(reader.snapshot().revision, valid.revision);
    let receipt = writer
        .save(&term, &config.master_key_meta().unwrap(), keys(300))
        .await
        .unwrap();
    await_revision(&reader, receipt.revision).await;
    assert_eq!(reader.snapshot().keys.as_ref().unwrap().current_key_id, 300);
    let row = store
        .read(Read::range(
            crate::ENCRYPTION_KEYS_PATH,
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap()
        .entries
        .remove(0);
    assert_eq!(reader.snapshot().revision, row.mod_revision);
    watch.shutdown().await.unwrap();
    await_native_watchers(&fixture.endpoint, 0).await;
    term.resign().await.unwrap();
    println!(
        "Native compaction {header} recovered without advancing accepted key revision; corrupt/delete/repair converged and watcher gauge returned to zero"
    );
}
#[tokio::test]
async fn mcs_014_disconnect_reconnect_and_cancel_pending_io() {
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let config = policy(file.to_str().unwrap());
    let mut fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Leadership::campaign(
        store.clone(),
        CancellationToken::new(),
        1402,
        b"key-owner".to_vec(),
        90,
    )
    .await
    .unwrap();
    let writer = DictionaryStorage::new(store.clone(), Arc::new(ProtoKeyDictionaryCodec));
    writer
        .save(&term, &config.master_key_meta().unwrap(), keys(100))
        .await
        .unwrap();
    let reader = Arc::new(DictionaryStorage::new(
        store,
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    let rotation = Arc::new(KeyRotation::new(reader.clone(), &config).unwrap());
    let watch = KeyWatch::start(
        &tokio::runtime::Handle::current(),
        rotation,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    await_native_watchers(&fixture.endpoint, 1).await;
    fixture.command("stop", "");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if watch.last_failure().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    fixture.command("start", "");
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if writer
                .backend()
                .read(astersql_pd_storage_domain::Read::range(
                    crate::ENCRYPTION_KEYS_PATH,
                    b"",
                    astersql_pd_storage_domain::Consistency::Linearizable,
                ))
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("existing unary client must reconnect before the next fixture write");
    let receipt = writer
        .save(&term, &config.master_key_meta().unwrap(), keys(200))
        .await
        .unwrap();
    await_revision(&reader, receipt.revision).await;
    assert_eq!(reader.snapshot().keys.as_ref().unwrap().current_key_id, 200);
    await_native_watchers(&fixture.endpoint, 1).await;
    let paused = fixture.suspend();
    tokio::time::timeout(Duration::from_millis(500), watch.shutdown())
        .await
        .expect("shutdown must cancel actual pending transport IO")
        .unwrap();
    let frozen = reader.snapshot();
    drop(paused);
    writer
        .save(&term, &config.master_key_meta().unwrap(), keys(300))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(reader.snapshot().revision, frozen.revision);
    await_native_watchers(&fixture.endpoint, 0).await;
    term.resign().await.unwrap();
}
#[tokio::test]
async fn mcs_014_timer_uses_live_rotation_and_parent_cancellation() {
    use std::sync::atomic::{AtomicI64, Ordering};
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let mut config = policy(file.to_str().unwrap());
    config.data_key_rotation_period = astersql_pd_typeutil::Duration::from_nanos(100_000_000);
    let fixture = fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let term = Arc::new(
        Leadership::campaign(
            store.clone(),
            CancellationToken::new(),
            1403,
            b"key-owner".to_vec(),
            60,
        )
        .await
        .unwrap(),
    );
    let storage = Arc::new(DictionaryStorage::new(
        store.clone(),
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    storage
        .save(&term, &config.master_key_meta().unwrap(), keys(100))
        .await
        .unwrap();
    let now = Arc::new(AtomicI64::new(1601679533_000_000_000));
    let value = now.clone();
    let rotation = Arc::new(
        KeyRotation::new_with_resources(
            storage.clone(),
            &config,
            Arc::new(move || i128::from(value.load(Ordering::SeqCst))),
            rand::rngs::OsRng,
        )
        .unwrap(),
    );
    rotation.set_leadership(Some(term.clone())).await.unwrap();
    let parent = CancellationToken::new();
    let watch = KeyWatch::start(
        &tokio::runtime::Handle::current(),
        rotation.clone(),
        &parent,
    )
    .await
    .unwrap();
    let before = storage.snapshot();
    now.fetch_add(1_000_000_000, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if storage.snapshot().revision > before.revision {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let after = storage.snapshot();
    assert_eq!(after.keys.as_ref().unwrap().keys.len(), 2);
    assert_ne!(after.keys.as_ref().unwrap().current_key_id, 100);
    rotation.set_leadership(None).await.unwrap();
    parent.cancel();
    watch.shutdown().await.unwrap();
    let frozen = storage.snapshot();
    let writer = DictionaryStorage::new(store, Arc::new(ProtoKeyDictionaryCodec));
    writer
        .save(&term, &config.master_key_meta().unwrap(), keys(300))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(storage.snapshot().revision, frozen.revision);
    await_native_watchers(&fixture.endpoint, 0).await;
    term.resign().await.unwrap();
}

async fn go_reloaded(go: &astersql_pd_compat_tests::harness::Server, revision: i64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if go
                .logs()
                .lines()
                .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .any(|v| {
                    v["message"] == "reloaded encryption keys"
                        && v["revision"].as_i64() == Some(revision)
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
async fn encode(config: &EncryptionConfig, keys: &KeyDictionary) -> Vec<u8> {
    use crate::{KeyDictionaryCodec, KeyEnvelope, MasterKey};
    let codec = ProtoKeyDictionaryCodec;
    let meta = config.master_key_meta().unwrap();
    let master = MasterKey::from_config(Some(&meta), &[]).await.unwrap();
    let (content, iv) = master.encrypt(&codec.encode_keys(keys).unwrap()).unwrap();
    codec
        .encode_envelope(&KeyEnvelope {
            content,
            iv,
            master_key: Some(meta),
            ciphertext_key: master.ciphertext_key().to_vec(),
        })
        .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_014_go_and_rust_watch_same_real_revisions() {
    use astersql_pd_storage_domain::Mutation;
    let directory = crate::master_key_file_test::Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, crate::master_key_file_test::KEY).unwrap();
    let mut config = policy(file.to_str().unwrap());
    config.data_encryption_method = "aes256-ctr".into();
    let mut go = crate::master_key_file_test::start_go(&directory, "file", file.to_str().unwrap());
    let (store, _) = crate::master_key_file_test::go_content(&mut go).await;
    let reader = Arc::new(DictionaryStorage::new(
        store.clone(),
        Arc::new(ProtoKeyDictionaryCodec),
    ));
    let rotation = Arc::new(KeyRotation::new(reader.clone(), &config).unwrap());
    let watch = KeyWatch::start(
        &tokio::runtime::Handle::current(),
        rotation,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let original = reader.snapshot();
    let mut updated = original.keys.as_ref().unwrap().as_ref().clone();
    let id = (300..).find(|id| !updated.keys.contains_key(id)).unwrap();
    updated.keys.insert(
        id,
        DataKey {
            key: vec![2; 32],
            method: EncryptionMethod::Aes256Ctr,
            creation_time: 1601679533,
            was_exposed: false,
        },
    );
    updated.current_key_id = id;
    let receipt = store
        .transact(
            vec![],
            vec![Mutation::put(
                crate::ENCRYPTION_KEYS_PATH,
                &encode(&config, &updated).await,
            )],
        )
        .await
        .unwrap();
    await_revision(&reader, receipt.revision).await;
    go_reloaded(&go, receipt.revision).await;
    assert_eq!(reader.snapshot().keys.as_deref(), Some(&updated));
    assert_eq!(original.keys.as_ref().unwrap().keys.len(), 1);
    store
        .transact(
            vec![],
            vec![Mutation::put(crate::ENCRYPTION_KEYS_PATH, &[0xff])],
        )
        .await
        .unwrap();
    await_failure(&watch, crate::KeyWatchPhase::Decode).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if go.logs().contains("ErrProtoUnmarshal") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(reader.snapshot().revision, receipt.revision);
    let repaired = store
        .transact(
            vec![],
            vec![Mutation::put(
                crate::ENCRYPTION_KEYS_PATH,
                &encode(&config, &updated).await,
            )],
        )
        .await
        .unwrap();
    await_revision(&reader, repaired.revision).await;
    go_reloaded(&go, repaired.revision).await;
    watch.shutdown().await.unwrap();
    let frozen = reader.snapshot();
    let after = store
        .transact(
            vec![],
            vec![Mutation::put(
                crate::ENCRYPTION_KEYS_PATH,
                &encode(&config, &updated).await,
            )],
        )
        .await
        .unwrap();
    go_reloaded(&go, after.revision).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(reader.snapshot().revision, frozen.revision);
    assert!(!go.logs().contains(crate::master_key_file_test::KEY));
    println!(
        "Go/Rust watch accepted revisions {} and {}, both rejected corrupt protobuf; after Rust shutdown only Go accepted {}",
        receipt.revision, repaired.revision, after.revision
    );
    go.stop().unwrap();
}

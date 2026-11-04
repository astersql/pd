// Copyright 2026 AsterSQL.
use crate::{MasterKey, MasterKeyFileConfig};
use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
use astersql_pd_proto::encryptionpb::{EncryptedContent, KeyDictionary};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use astersql_pd_storage_kv::EtcdMetaStore;
use prost::Message;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
pub(super) const KEY: &str = "2f07ec61e5a50284f47f2b402a962ec672e500b26cb3aa568bb1531300c74806";
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) struct Directory(pub(super) PathBuf);
impl Directory {
    pub(super) fn new() -> Self {
        let p = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join(format!(
            "mcs010-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) fn start_go(dir: &Directory, kind: &str, path: &str) -> Server {
    let config = dir.0.join("pd.toml");
    std::fs::write(&config,format!("[security.encryption]\ndata-encryption-method = 'aes256-ctr'\n[security.encryption.master-key]\ntype = '{kind}'\npath = {}\n[log]\nformat = 'json'\n",serde_json::to_string(path).unwrap())).unwrap();
    Server::start_with_args(
        &pinned_go_binary().unwrap(),
        &[&format!("--config={}", config.display())],
    )
    .unwrap()
}
pub(super) async fn go_content(go: &mut Server) -> (Arc<dyn MetaStore>, EncryptedContent) {
    go.ready_with_timeout(Duration::from_secs(12)).unwrap();
    let store: Arc<dyn MetaStore> = Arc::new(
        EtcdMetaStore::connect(std::slice::from_ref(&go.url), Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let content = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let rows = store
                .read(Read::prefix(b"encryption_keys", Consistency::Linearizable))
                .await
                .unwrap();
            if let Some(row) = rows.entries.first() {
                break EncryptedContent::decode(row.value.as_slice()).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    (store, content)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_010_behavior() {
    let directory = Directory::new();
    let file = directory.0.join("master");
    std::fs::write(&file, format!(" \t\r\n{KEY}\u{a0}\u{3000}")).unwrap();
    let config = MasterKeyFileConfig {
        file_path: file.to_str().unwrap().into(),
    };
    let mut go = start_go(&directory, "file", &config.file_path);
    let (backend, mut content) = go_content(&mut go).await;
    assert_eq!(content.iv.len(), 12);
    assert!(content.content.len() > 16);
    assert!(content.ciphertext_key.is_empty());
    println!(
        "Go file master-key record: encrypted_bytes={}, iv_bytes={}, ciphertext_key_bytes={}",
        content.content.len(),
        content.iv.len(),
        content.ciphertext_key.len()
    );
    let key = MasterKey::from_file(Some(&config)).unwrap();
    assert!(
        !key.is_plaintext(),
        "Go file backend must not become plaintext"
    );
    let plain = key.decrypt(&content.content, &content.iv).unwrap();
    let dict = KeyDictionary::decode(plain.as_slice()).unwrap();
    let data = dict.keys.get(&dict.current_key_id).unwrap();
    assert_eq!(data.key.len(), 32);
    assert!(!data.was_exposed);
    let (cipher, iv) = key.encrypt(&plain).unwrap();
    assert_eq!(key.decrypt(&cipher, &iv).unwrap(), plain);
    let mut bad = content.content.clone();
    bad[0] ^= 1;
    assert_eq!(
        key.decrypt(&bad, &content.iv).unwrap_err().code(),
        "PD:encryption:ErrEncryptionGCMDecrypt"
    );
    // Re-wrap the real Go dictionary and require the real Go watcher to decrypt it.
    content.content = cipher;
    content.iv = iv;
    let commit = backend
        .transact(
            vec![],
            vec![astersql_pd_storage_domain::Mutation::put(
                b"encryption_keys",
                &content.encode_to_vec(),
            )],
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if go
                .logs()
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .any(|v| {
                    v["message"] == "reloaded encryption keys"
                        && v["revision"].as_i64() == Some(commit.revision)
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("Go did not reload Rust ciphertext: {}", go.logs()));
    content.content[0] ^= 1;
    backend
        .transact(
            vec![],
            vec![astersql_pd_storage_domain::Mutation::put(
                b"encryption_keys",
                &content.encode_to_vec(),
            )],
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if go
                .logs()
                .contains("[PD:encryption:ErrEncryptionGCMDecrypt]authentication fail")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    println!(
        "Go accepted Rust file-key wrapping at revision {}; both reject corrupt authentication",
        commit.revision
    );
    assert!(!format!("{key:?}").contains(KEY));
    assert!(!go.logs().contains(KEY));
    go.stop().unwrap();
}
#[test]
fn mcs_010_plaintext_and_invalid_file() {
    let key = MasterKey::plaintext();
    assert!(key.is_plaintext());
    assert!(key.ciphertext_key().is_empty());
    let (cipher, iv) = key.encrypt(b"plaintext\0\xff").unwrap();
    assert!(iv.is_empty());
    assert_eq!(
        key.decrypt(&cipher, b"ignored nonce").unwrap(),
        b"plaintext\0\xff"
    );
    assert!(MasterKey::from_file(None).is_err());
    assert!(MasterKey::from_file(Some(&MasterKeyFileConfig::default())).is_err());
    let directory = Directory::new();
    let config = MasterKeyFileConfig {
        file_path: directory.0.join("missing").to_str().unwrap().into(),
    };
    assert!(MasterKey::from_file(Some(&config)).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_010_plaintext_matches_real_go() {
    let directory = Directory::new();
    let mut go = start_go(&directory, "plaintext", "ignored");
    let (_, content) = go_content(&mut go).await;
    assert!(content.iv.is_empty());
    assert!(content.ciphertext_key.is_empty());
    let key = MasterKey::plaintext();
    let plain = key
        .decrypt(&content.content, b"ignored invalid nonce")
        .unwrap();
    let dict = KeyDictionary::decode(plain.as_slice()).unwrap();
    assert!(dict.keys.get(&dict.current_key_id).unwrap().was_exposed);
    assert_eq!(key.encrypt(&plain).unwrap(), (content.content, vec![]));
    assert!(!go.logs().contains(KEY));
    go.stop().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_010_file_errors_match_real_go() {
    // None means physically missing; byte cases exercise real file reads.
    for (label, body) in [
        ("missing", None),
        ("not-hex", Some(b"not-a-hex-string".to_vec())),
        ("odd", Some(b"000".to_vec())),
        ("odd-invalid", Some(b"00g".to_vec())),
        ("short", Some(b"00".to_vec())),
        ("invalid-utf8", Some(vec![0xff])),
    ] {
        let directory = Directory::new();
        let file = directory.0.join("key");
        if let Some(body) = &body {
            std::fs::write(&file, body).unwrap();
        }
        let config = MasterKeyFileConfig {
            file_path: file.to_str().unwrap().into(),
        };
        let rust = MasterKey::from_file(Some(&config)).unwrap_err().to_string();
        let mut go = start_go(&directory, "file", &config.file_path);
        let observed = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Some(error) = go
                    .logs()
                    .lines()
                    .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                    .find_map(|v| {
                        v["error"]
                            .as_str()
                            .filter(|s| s.contains("[PD:encryption:ErrEncryptionNewMasterKey]"))
                            .map(str::to_owned)
                    })
                {
                    break error;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(rust, observed, "actual Go file error {label}");
        assert!(!observed.contains(KEY));
        assert!(!rust.contains(KEY));
        println!("Go/Rust master file {label}: {observed}");
        go.stop().unwrap();
    }
}

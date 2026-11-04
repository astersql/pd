// Copyright 2026 AsterSQL.
use crate::MasterKey;
use aws_credential_types::Credentials;
use aws_sdk_kms::{
    Client,
    config::{BehaviorVersion, Region, retry::RetryConfig},
};
use aws_smithy_http_client::test_util::{ReplayEvent, StaticReplayClient};
use aws_smithy_types::body::SdkBody;
use axum::http::{Request, Response};
fn replay(response: &str) -> (Client, StaticReplayClient) {
    let http = StaticReplayClient::new(vec![ReplayEvent::new(
        Request::builder()
            .uri("https://kms.us-east-2.amazonaws.com/")
            .body(SdkBody::empty())
            .unwrap(),
        Response::builder()
            .status(200)
            .header("content-type", "application/x-amz-json-1.1")
            .body(SdkBody::from(response))
            .unwrap(),
    )]);
    let client = Client::from_conf(
        aws_sdk_kms::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-2"))
            .credentials_provider(Credentials::for_tests())
            .retry_config(RetryConfig::disabled())
            .http_client(http.clone())
            .build(),
    );
    (client, http)
}
#[tokio::test]
async fn mcs_011_behavior() {
    let (client, http) = replay(
        r#"{"Plaintext":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","CiphertextBlob":"b3BhcXVl"}"#,
    );
    let key = MasterKey::from_kms_client(&client, "alias/public-test", &[])
        .await
        .unwrap();
    assert!(
        !key.is_plaintext(),
        "KMS must return an authenticated master key"
    );
    assert_eq!(key.ciphertext_key(), b"opaque");
    let requests: Vec<_> = http.actual_requests().collect();
    let request = requests.first().unwrap();
    assert_eq!(
        request.headers().get("x-amz-target"),
        Some("TrentService.GenerateDataKey")
    );
    let body: serde_json::Value = serde_json::from_slice(request.body().bytes().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"KeyId":"alias/public-test","NumberOfBytes":32})
    );
    let (cipher, iv) = key.encrypt(b"dictionary").unwrap();
    assert_eq!(key.decrypt(&cipher, &iv).unwrap(), b"dictionary");
}

#[tokio::test]
async fn mcs_011_decrypt_and_lengths() {
    let response = r#"{"Plaintext":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","CiphertextBlob":"aWdub3Jl"}"#;
    let (client, http) = replay(response);
    let key = MasterKey::from_kms_client(&client, "alias/public-test", b"saved")
        .await
        .unwrap();
    assert_eq!(key.ciphertext_key(), b"saved");
    let requests: Vec<_> = http.actual_requests().collect();
    assert_eq!(
        requests[0].headers().get("x-amz-target"),
        Some("TrentService.Decrypt")
    );
    let body: serde_json::Value =
        serde_json::from_slice(requests[0].body().bytes().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"KeyId":"alias/public-test","CiphertextBlob":"c2F2ZWQ="})
    );
    assert!(
        requests[0]
            .headers()
            .get("authorization")
            .unwrap()
            .contains("/us-east-2/kms/aws4_request")
    );
    for ciphertext in [&b""[..], &b"saved"[..]] {
        for (response, length) in [(r#"{"Plaintext":"AA=="}"#, 1), ("{}", 0)] {
            let (client, _) = replay(response);
            let err = MasterKey::from_kms_client(&client, "alias/public-test", ciphertext)
                .await
                .unwrap_err();
            let word = if ciphertext.is_empty() {
                "generated"
            } else {
                "decrypted"
            };
            assert_eq!(
                err.to_string(),
                format!(
                    "[PD:ErrEncryptionKMS]unexpected data key length {word} from AWS KMS, expected 32 vs actual {length}"
                )
            );
        }
    }
}
#[tokio::test]
async fn mcs_011_errors_and_redaction() {
    use crate::MasterKeyMeta;
    assert_eq!(
        MasterKey::from_config(None, &[])
            .await
            .unwrap_err()
            .to_string(),
        "[PD:encryption:ErrEncryptionNewMasterKey]master key config is empty"
    );
    let meta = MasterKeyMeta::Kms {
        vendor: "OTHER".into(),
        key_id: String::new(),
        region: String::new(),
        endpoint: String::new(),
    };
    assert_eq!(
        MasterKey::from_config(Some(&meta), &[])
            .await
            .unwrap_err()
            .to_string(),
        "[PD:ErrEncryptionKMS]unsupported KMS vendor: OTHER"
    );
    for ciphertext in [&b""[..], &b"saved"[..]] {
        let http = StaticReplayClient::new(vec![ReplayEvent::new(
            Request::builder().uri("https://kms.us-east-2.amazonaws.com/").body(SdkBody::empty()).unwrap(),
            Response::builder().status(400).header("content-type","application/x-amz-json-1.1")
                .body(SdkBody::from(r#"{"__type":"AccessDeniedException","message":"DO_NOT_LOG_CREDENTIAL_OR_KEY"}"#)).unwrap(),
        )]);
        let client = Client::from_conf(
            aws_sdk_kms::Config::builder()
                .behavior_version(BehaviorVersion::latest())
                .region(Region::new("us-east-2"))
                .credentials_provider(Credentials::for_tests())
                .retry_config(RetryConfig::disabled())
                .http_client(http)
                .build(),
        );
        let error = MasterKey::from_kms_client(&client, "alias/public-test", ciphertext)
            .await
            .unwrap_err();
        let operation = if ciphertext.is_empty() {
            "generate"
        } else {
            "decrypt"
        };
        assert_eq!(
            error.to_string(),
            format!(
                "[PD:ErrEncryptionKMS]fail to {operation} data key from AWS KMS: AccessDeniedException"
            )
        );
        assert!(!format!("{error:?}").contains("DO_NOT_LOG"));
    }
}

// Existing Go binaries cannot link Rust test utilities. This loopback transport
// adapts their real SDK HTTP requests to the official Smithy StaticReplayClient.
async fn bridge_request(
    axum::extract::State(replay): axum::extract::State<StaticReplayClient>,
    request: axum::extract::Request,
) -> axum::response::Response {
    use aws_smithy_runtime_api::client::http::HttpConnector;
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 1024 * 1024).await.unwrap();
    let request: aws_smithy_runtime_api::client::orchestrator::HttpRequest =
        Request::from_parts(parts, SdkBody::from(bytes))
            .try_into()
            .unwrap();
    let response = replay.call(request).await.unwrap();
    let mut output = Response::builder().status(response.status().as_u16());
    for (key, value) in response.headers().iter() {
        output = output.header(key, value);
    }
    output
        .body(axum::body::Body::from(
            response.body().bytes().unwrap().to_vec(),
        ))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_011_go_dictionary_and_default_chain() {
    use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
    use astersql_pd_proto::encryptionpb::{EncryptedContent, KeyDictionary};
    use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
    use astersql_pd_storage_kv::EtcdMetaStore;
    use prost::Message;
    use std::{path::PathBuf, time::Duration};
    let replay = StaticReplayClient::new((0..32).map(|_|ReplayEvent::new(
        Request::builder().uri("http://localhost/").body(SdkBody::empty()).unwrap(),
        Response::builder().status(200).header("content-type","application/x-amz-json-1.1")
            .body(SdkBody::from(r#"{"Plaintext":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","CiphertextBlob":"b3BhcXVl"}"#)).unwrap()
    )).collect());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let sts = StaticReplayClient::new((0..16).map(|_|ReplayEvent::new(
        Request::builder().uri("http://localhost/sts").body(SdkBody::empty()).unwrap(),
        Response::builder().status(200).header("content-type","text/xml")
            .body(SdkBody::from(r#"<AssumeRoleWithWebIdentityResponse xmlns="https://sts.amazonaws.com/doc/2011-06-15/"><AssumeRoleWithWebIdentityResult><Credentials><AccessKeyId>STS_PUBLIC_TEST</AccessKeyId><SecretAccessKey>STS_SECRET_PUBLIC_TEST</SecretAccessKey><SessionToken>STS_SESSION_PUBLIC_TEST</SessionToken><Expiration>2099-01-01T00:00:00Z</Expiration></Credentials><AssumedRoleUser><Arn>arn:aws:sts::123456789012:assumed-role/test/session</Arn><AssumedRoleId>test:session</AssumedRoleId></AssumedRoleUser><SubjectFromWebIdentityToken>subject</SubjectFromWebIdentityToken></AssumeRoleWithWebIdentityResult><ResponseMetadata><RequestId>public-test</RequestId></ResponseMetadata></AssumeRoleWithWebIdentityResponse>"#)).unwrap()
    )).collect());
    let app = axum::Router::new()
        .route(
            "/",
            axum::routing::post(bridge_request).with_state(replay.clone()),
        )
        .route(
            "/sts",
            axum::routing::post(bridge_request).with_state(sts.clone()),
        )
        .route(
            "/sts/",
            axum::routing::post(bridge_request).with_state(sts.clone()),
        );
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    struct ServerTask(tokio::task::JoinHandle<()>);
    impl Drop for ServerTask {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _task = ServerTask(task);
    let directory = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join(format!("mcs011-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _directory = Directory(directory.clone());
    let config = directory.join("pd.toml");
    // Go persists this endpoint string but intentionally never uses it for KMS.
    std::fs::write(&config,"[security.encryption]\ndata-encryption-method='aes256-ctr'\n[security.encryption.master-key]\ntype='kms'\nkey-id='alias/public-test'\nregion='us-east-2'\nendpoint='http://127.0.0.1:1/ignored'\n[log]\nformat='json'\n").unwrap();
    let env = vec![
        ("AWS_ENDPOINT_URL_KMS".into(), endpoint.clone()),
        ("AWS_ACCESS_KEY_ID".into(), "AKID_PUBLIC_TEST".into()),
        ("AWS_SECRET_ACCESS_KEY".into(), "SECRET_PUBLIC_TEST".into()),
        ("AWS_EC2_METADATA_DISABLED".into(), "true".into()),
        ("AWS_ROLE_ARN".into(), String::new()),
        ("AWS_WEB_IDENTITY_TOKEN_FILE".into(), String::new()),
    ];
    let mut go = Server::start_with_environment(
        &pinned_go_binary().unwrap(),
        &[&format!("--config={}", config.display())],
        &env,
    )
    .unwrap();
    go.ready_with_timeout(Duration::from_secs(12)).unwrap();
    let store = EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
        .await
        .unwrap();
    let mut content = tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            let rows = store
                .read(Read::prefix(b"encryption_keys", Consistency::Linearizable))
                .await
                .unwrap();
            if let Some(row) = rows.entries.first() {
                break EncryptedContent::decode(row.value.as_slice()).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(content.ciphertext_key, b"opaque");
    let (client, _) =
        super::kms_test::replay(r#"{"Plaintext":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}"#);
    let key = MasterKey::from_kms_client(&client, "alias/public-test", &content.ciphertext_key)
        .await
        .unwrap();
    let plain = key.decrypt(&content.content, &content.iv).unwrap();
    let dict = KeyDictionary::decode(plain.as_slice()).unwrap();
    assert_eq!(dict.keys[&dict.current_key_id].key.len(), 32);
    assert!(!dict.keys[&dict.current_key_id].was_exposed);
    let (cipher, iv) = key.encrypt(&plain).unwrap();
    content.content = cipher;
    content.iv = iv;
    let receipt = store
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
                .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .any(|v| {
                    v["message"] == "reloaded encryption keys"
                        && v["revision"].as_i64() == Some(receipt.revision)
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    println!(
        "Go accepted Rust KMS wrapping at revision {}",
        receipt.revision
    );
    // Test the production default credential chain in a child, avoiding unsafe
    // process-global environment mutation in a multithreaded Rust test runtime.
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args([
            "--exact",
            "kms_test::kms_default_chain_child",
            "--nocapture",
        ])
        .envs(env.iter().map(|(k, v)| (k, v)))
        .env("PD_KMS_CHILD", "1");
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SECRET_PUBLIC_TEST"));
    let targets: Vec<_> = replay
        .actual_requests()
        .map(|r| {
            let auth = r.headers().get("authorization").unwrap();
            assert!(auth.contains("AKID_PUBLIC_TEST/"));
            assert!(auth.contains("/us-east-2/kms/aws4_request"));
            let body: serde_json::Value =
                serde_json::from_slice(r.body().bytes().unwrap()).unwrap();
            assert_eq!(body["KeyId"], "alias/public-test");
            let target = r.headers().get("x-amz-target").unwrap().to_owned();
            if target.ends_with("GenerateDataKey") {
                assert_eq!(body["NumberOfBytes"], 32);
            } else {
                assert_eq!(body["CiphertextBlob"], "b3BhcXVl");
            }
            target
        })
        .collect();
    assert!(targets.iter().any(|s| s.ends_with("GenerateDataKey")));
    assert!(targets.iter().any(|s| s.ends_with("Decrypt")));
    println!(
        "Go KMS dictionary decrypted by Rust; production default chain signed region us-east-2 via SDK endpoint; {} official replay requests",
        targets.len()
    );
    assert!(!go.logs().contains("SECRET_PUBLIC_TEST"));
    go.stop().unwrap();
    let token = directory.join("public-web-token");
    std::fs::write(&token, "PUBLIC_WEB_TOKEN").unwrap();
    let mut web_env = env.clone();
    for (k, v) in &mut web_env {
        if k == "AWS_ROLE_ARN" {
            *v = "arn:aws:iam::123456789012:role/public-test".into();
        }
        if k == "AWS_WEB_IDENTITY_TOKEN_FILE" {
            *v = token.to_str().unwrap().into();
        }
    }
    web_env.push(("AWS_ROLE_SESSION_NAME".into(), "public-session".into()));
    web_env.push(("AWS_ENDPOINT_URL_STS".into(), format!("{endpoint}/sts")));
    let old_requests = replay.actual_requests().count();
    let mut go = Server::start_with_environment(
        &pinned_go_binary().unwrap(),
        &[&format!("--config={}", config.display())],
        &web_env,
    )
    .unwrap();
    go.ready_with_timeout(Duration::from_secs(12)).unwrap();
    let store = EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let rows = store
                .read(Read::prefix(b"encryption_keys", Consistency::Linearizable))
                .await
                .unwrap();
            if !rows.entries.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "kms_test::kms_default_chain_child",
            "--nocapture",
        ])
        .envs(web_env.iter().map(|(k, v)| (k, v)))
        .env("PD_KMS_CHILD", "1")
        .output()
        .unwrap();
    if !output.status.success() {
        println!(
            "STS requests: {:?}",
            sts.actual_requests()
                .map(|r| (
                    r.uri().to_string(),
                    r.headers().get("content-type").map(str::to_owned)
                ))
                .collect::<Vec<_>>()
        );
        println!("KMS signed requests: {}", replay.actual_requests().count());
    }
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut calls = 0;
    for request in replay.actual_requests().skip(old_requests) {
        assert!(
            request
                .headers()
                .get("authorization")
                .unwrap()
                .contains("Credential=STS_PUBLIC_TEST/")
        );
        calls += 1;
    }
    assert!(calls >= 3);
    let mut sts_calls = 0;
    for request in sts.actual_requests() {
        let body = std::str::from_utf8(request.body().bytes().unwrap()).unwrap();
        assert!(body.contains("AssumeRoleWithWebIdentity"));
        assert!(body.contains("PUBLIC_WEB_TOKEN"));
        assert!(body.contains("public-session"));
        assert!(body.contains("123456789012"));
        sts_calls += 1;
    }
    assert!(sts_calls >= 3);
    println!(
        "Go and Rust web identity override environment credentials: {sts_calls} STS and {calls} KMS official replay requests"
    );
    for secret in [
        "SECRET_PUBLIC_TEST",
        "STS_SECRET_PUBLIC_TEST",
        "PUBLIC_WEB_TOKEN",
    ] {
        assert!(!go.logs().contains(secret));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    }
    go.stop().unwrap();
    let requests_before = replay.actual_requests().count();
    for (k, v) in &mut web_env {
        if k == "AWS_WEB_IDENTITY_TOKEN_FILE" {
            *v = directory.join("missing-token").to_str().unwrap().into();
        }
    }
    let go = Server::start_with_environment(
        &pinned_go_binary().unwrap(),
        &[&format!("--config={}", config.display())],
        &web_env,
    )
    .unwrap();
    let expected = "[PD:ErrEncryptionKMS]fail to generate data key from AWS KMS";
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            if go.logs().contains(expected) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "kms_test::kms_default_chain_child",
            "--nocapture",
        ])
        .envs(web_env.iter().map(|(k, v)| (k, v)))
        .env("PD_KMS_CHILD", "missing")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        requests_before,
        replay.actual_requests().count(),
        "missing web token must not fall back to environment credentials"
    );
    assert!(!go.logs().contains("SECRET_PUBLIC_TEST"));
    println!(
        "Go/Rust missing web token both fail KMS generation without falling back to environment credentials"
    );
    drop(go);
}
#[tokio::test]
async fn kms_default_chain_child() {
    let mode = std::env::var("PD_KMS_CHILD").unwrap_or_default();
    if mode.is_empty() {
        return;
    }
    let meta = crate::MasterKeyMeta::Kms {
        vendor: "AWS".into(),
        key_id: "alias/public-test".into(),
        region: "us-east-2".into(),
        endpoint: "http://127.0.0.1:1/ignored".into(),
    };
    if mode == "missing" {
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            MasterKey::from_config(Some(&meta), &[]),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("[PD:ErrEncryptionKMS]fail to generate data key from AWS KMS")
        );
        assert!(!format!("{error:?}").contains("SECRET_PUBLIC_TEST"));
        return;
    }
    let key = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        MasterKey::from_config(Some(&meta), &[]),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(key.ciphertext_key(), b"opaque");
    let restored = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        MasterKey::from_config(Some(&meta), key.ciphertext_key()),
    )
    .await
    .unwrap()
    .unwrap();
    let (cipher, iv) = key.encrypt(b"shared dictionary").unwrap();
    assert_eq!(
        restored.decrypt(&cipher, &iv).unwrap(),
        b"shared dictionary"
    );
}

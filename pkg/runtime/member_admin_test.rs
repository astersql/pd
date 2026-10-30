// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder, member_admin::EtcdMemberAdmin};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
pub(crate) fn request(url: &str, method: &str, path: &str, body: &str) -> (u16, serde_json::Value) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "8",
            "-X",
            method,
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    let (body, code) = text.rsplit_once('\n').unwrap();
    if path != "/pd/api/v1/ping" {
        if content == "text/plain; charset=utf-8" {
            assert_eq!(code, "500");
            assert_eq!(body, "[PD:server:ErrServerNotStarted]server not started\n");
        } else {
            assert_eq!(content, "application/json; charset=UTF-8");
        }
    }
    (
        code.parse().unwrap(),
        serde_json::from_str(body).unwrap_or(serde_json::Value::String(body.into())),
    )
}
async fn node(
    store: Arc<dyn MetaStore>,
    admin: Arc<EtcdMemberAdmin>,
    id: u64,
) -> (crate::PdNode, String, NodeConfig) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let config = NodeConfig {
        cluster_id: 7440,
        member_id: id,
        name: format!("member-{id}"),
        client_url: url.clone(),
        peer_url: format!("http://peer-{id}"),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .with_etcd_member_admin(admin)
        .start(
            store,
            config.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    (node, url, config)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_044_exclusive_binding_and_shared_refusal() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut client = etcd_client::Client::connect([fixture.endpoint.clone()], None)
        .await
        .unwrap();
    let id = client.status().await.unwrap().header().unwrap().member_id();
    let provider = EtcdMemberAdmin::connect(fixture.endpoint.clone(), id, Duration::from_secs(2))
        .await
        .unwrap();
    let diagnostic = crate::etcd_diagnostics::EtcdMemberDiagnostics::connect(
        fixture.endpoint.clone(),
        id,
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    diagnostic.bind(store.as_ref(), 7440, 2).await.unwrap();
    assert!(
        provider
            .bind(store.as_ref(), 7440, 1)
            .await
            .unwrap_err()
            .contains("shared diagnostic")
    );
    provider.bind(store.as_ref(), 7440, 2).await.unwrap();
    assert!(provider.bind(store.as_ref(), 7440, 1).await.is_err());
    assert!(provider.bind(store.as_ref(), 7441, 2).await.is_err());
    assert!(diagnostic.bind(store.as_ref(), 7440, 3).await.is_err());
    let (mut node, url, _) = node(store.clone(), Arc::new(provider), 2).await;
    let before = client.member_list().await.unwrap().members().to_vec();
    assert_eq!(
        request(&url, "DELETE", "/pd/api/v1/members/id/2", "").0,
        500
    );
    assert_eq!(before, client.member_list().await.unwrap().members());
    assert!(
        store
            .read(Read::prefix(
                b"/pd/7440/rust/removed-members/",
                Consistency::Linearizable
            ))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    node.shutdown().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires official standalone etcd via PD_ETCD_BIN"]
async fn task_044_remove_real_dedicated_member() {
    use crate::etcd_diagnostics_test::{address, launch, wait_status};
    let binary = std::env::var("PD_ETCD_BIN").unwrap();
    let url = address();
    let peer = address();
    let voter = launch(
        &binary,
        "voter",
        &url,
        &peer,
        &format!("voter={peer}"),
        false,
    );
    let mut client = wait_status(&url).await;
    let voter_id = client.status().await.unwrap().header().unwrap().member_id();
    let store: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage::EtcdMetaStore::connect(&[url.clone()], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let leader_admin = Arc::new(
        EtcdMemberAdmin::connect(url.clone(), voter_id, Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let gated = Arc::new(crate::member_admin_fault_test::MemberStore {
        inner: store.clone(),
        mode: std::sync::atomic::AtomicU8::new(0),
        entered: tokio::sync::Notify::new(),
    });
    let (mut leader, pd_url, _) = node(gated.clone(), leader_admin.clone(), 1).await;
    for by_name in [true, false] {
        let target_url = address();
        let target_peer = address();
        let response = client
            .member_add(
                [target_peer.clone()],
                Some(etcd_client::MemberAddOptions::new().with_is_learner()),
            )
            .await
            .unwrap();
        let id = response.member().unwrap().id();
        let process = launch(
            &binary,
            "target",
            &target_url,
            &target_peer,
            &format!("voter={peer},target={target_peer}"),
            true,
        );
        let _ = wait_status(&target_url).await;
        let pd_id = if by_name { 2 } else { 3 };
        let provider = Arc::new(
            EtcdMemberAdmin::connect(target_url, id, Duration::from_secs(2))
                .await
                .unwrap(),
        );
        let (mut target, target_pd_url, config) =
            node(store.clone(), provider.clone(), pd_id).await;
        assert_eq!(
            request(
                &pd_url,
                "POST",
                &format!("/pd/api/v1/members/name/member-{pd_id}"),
                r#"{"leader-priority":9}"#
            )
            .0,
            200
        );
        let before = client.member_list().await.unwrap().members().to_vec();
        assert_eq!(
            request(&pd_url, "DELETE", "/pd/api/v1/members/id/999", "").0,
            500
        );
        assert_eq!(before, client.member_list().await.unwrap().members());
        // A corrupt reverse binding must fail without touching real topology.
        let reverse = crate::member_admin::owner_key(
            client
                .status()
                .await
                .unwrap()
                .header()
                .unwrap()
                .cluster_id(),
            id,
        );
        store
            .transact(vec![], vec![Mutation::put(&reverse, b"other")])
            .await
            .unwrap();
        assert_eq!(
            request(
                &pd_url,
                "DELETE",
                &format!("/pd/api/v1/members/id/{pd_id}"),
                ""
            )
            .0,
            500
        );
        assert_eq!(before, client.member_list().await.unwrap().members());
        store
            .transact(
                vec![],
                vec![Mutation::put(&reverse, format!("7440:{pd_id}").as_bytes())],
            )
            .await
            .unwrap();
        let path = if by_name {
            format!("/pd/api/v1/members/name/member-{pd_id}")
        } else {
            format!("/pd/api/v1/members/id/{pd_id}")
        };
        if !by_name {
            gated.mode.store(3, std::sync::atomic::Ordering::SeqCst);
            assert_eq!(
                request(&pd_url, "DELETE", &path, "").0,
                500,
                "real etcd rejects oversized cleanup transaction"
            );
            assert!(
                !client
                    .member_list()
                    .await
                    .unwrap()
                    .members()
                    .iter()
                    .any(|m| m.id() == id)
            );
            assert_eq!(
                store
                    .read(Read::range(
                        format!("/pd/7440/rust/removed-members/{pd_id}").as_bytes(),
                        b"",
                        Consistency::Linearizable
                    ))
                    .await
                    .unwrap()
                    .entries[0]
                    .value,
                b"removing"
            );
        }
        let removed = request(&pd_url, "DELETE", &path, "");
        assert_eq!(removed.0, 200);
        if let Ok(path) = std::env::var("PD_HTTP_MEMBER_ORACLE") {
            let oracle: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(
                serde_json::json!(removed),
                oracle["responses"][if by_name { "remove_name" } else { "remove_id" }]
            );
        }
        assert!(
            !client
                .member_list()
                .await
                .unwrap()
                .members()
                .iter()
                .any(|m| m.id() == id)
        );
        tokio::time::timeout(Duration::from_secs(8), target.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(
            tokio::net::TcpStream::connect(target_pd_url.trim_start_matches("http://"))
                .await
                .is_err()
        );
        for key in [
            format!("/pd/7440/rust/members/{pd_id}"),
            format!("/pd/7440/member/{pd_id}/leader_priority"),
        ] {
            assert!(
                store
                    .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                    .await
                    .unwrap()
                    .entries
                    .is_empty()
            );
        }
        assert_eq!(
            request(
                &pd_url,
                "DELETE",
                &format!("/pd/api/v1/members/id/{pd_id}"),
                ""
            )
            .0,
            200,
            "retry reconciles a committed removal"
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        assert!(
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .with_etcd_member_admin(provider)
                .start(
                    store.clone(),
                    config,
                    listener,
                    Arc::new(prometheus::Registry::new())
                )
                .await
                .is_err()
        );
        assert_eq!(request(&pd_url, "GET", "/pd/api/v1/ping", "").0, 200);
        drop(process);
    }
    leader.shutdown().await.unwrap();
    drop(voter);
}

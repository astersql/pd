// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn call(url: &str, method: &str, path: &str, body: &str) -> (u16, String) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}",
            &format!("{url}/pd/api/v1/{path}"),
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (status.parse().unwrap(), body.into())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_080_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 779,
                member_id: 1,
                name: "limit".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let response = call(
        &url,
        "POST",
        "service-middleware/config/grpc-rate-limit",
        r#"{"label":"GetMembers","qps":0.0001}"#,
    );
    assert_eq!(response, (200, "gRPC limiter is updated.".into()));
    assert_eq!(call(&url, "GET", "health", "").0, 200);

    // The gRPC limiter rejects only the configured RPC.
    let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(url.clone())
        .await
        .unwrap();
    let members = client
        .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(members.members.len(), 1);
    let error = client
        .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::ResourceExhausted);
    node.shutdown().await.unwrap();
}

async fn observe(url: &str) -> serde_json::Value {
    use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
    use serde_json::json;
    let rate = "service-middleware/config/grpc-rate-limit";
    let mut observations = vec![];
    for (body, expected) in [
        (r#"{}"#, 400),
        (r#"{"label":"missing"}"#, 400),
        (r#"{"label":"GetMembers"}"#, 200),
        (r#"{"label":"GetMembers","qps":0.0001}"#, 200),
    ] {
        let result = call(url, "POST", rate, body);
        assert_eq!(result.0, expected);
        observations.push(json!(result));
    }
    let mut client = PdClient::connect(url.to_owned()).await.unwrap();
    let response = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    let id = response.header.unwrap().cluster_id;
    assert!(!response.members.is_empty());
    let err = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::ResourceExhausted);
    observations.push(json!([err.code() as i32, err.message()]));
    // HTTP admission is independent; disabling and enabling preserves the spent QPS bucket.
    assert_eq!(call(url, "GET", "health", "").0, 200);
    assert_eq!(
        call(
            url,
            "POST",
            "service-middleware/config",
            r#"{"enable-grpc-rate-limit":"false"}"#
        )
        .0,
        200
    );
    client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap();
    assert_eq!(
        call(
            url,
            "POST",
            "service-middleware/config",
            r#"{"grpc-rate-limit.enable-grpc-rate-limit":"true"}"#
        )
        .0,
        200
    );
    assert_eq!(
        client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::ResourceExhausted
    );
    observations.push(json!(call(
        url,
        "POST",
        rate,
        r#"{"label":"GetMembers","qps":0}"#
    )));
    client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap();
    assert_eq!(
        call(
            url,
            "POST",
            rate,
            r#"{"label":"QueryRegion","concurrency":1}"#
        )
        .0,
        200
    );
    let request = pdpb::QueryRegionRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: id,
            ..Default::default()
        }),
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::mpsc::channel(2);
    tx.send(request.clone()).await.unwrap();
    let mut first = client
        .query_region(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let reply = first.message().await.unwrap().unwrap();
    assert_eq!(
        reply.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    let second = client
        .query_region(tokio_stream::iter([request.clone()]))
        .await;
    let err = match second {
        Err(e) => e,
        Ok(r) => r.into_inner().message().await.unwrap_err(),
    };
    assert_eq!(err.code(), tonic::Code::ResourceExhausted);
    observations.push(json!([err.code() as i32, err.message()]));
    drop(first);
    drop(tx);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match client
            .query_region(tokio_stream::iter([request.clone()]))
            .await
        {
            Ok(r) => match r.into_inner().message().await {
                Ok(Some(_)) => break,
                Err(e) if e.code() == tonic::Code::ResourceExhausted => {}
                other => panic!("unexpected readmission: {other:?}"),
            },
            Err(e) => assert_eq!(e.code(), tonic::Code::ResourceExhausted),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "cancel leaked permit"
        );
        tokio::task::yield_now().await;
    }
    let config = call(url, "GET", "service-middleware/config", "");
    assert_eq!(config.0, 200);
    json!({"responses":observations,"config":serde_json::from_str::<serde_json::Value>(&config.1).unwrap(),"cancel_released":true})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_080_wire_and_cancellation() {
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    let cfg = NodeConfig {
        cluster_id: 7801,
        member_id: 1,
        name: "grpc-limit".into(),
        client_url: url.clone(),
        peer_url: "http://peer".into(),
    };
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            cfg.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let observation = observe(&url).await;
    if let Ok(go) = std::env::var("PD_GRPC_LIMIT_GO_URL") {
        let go_observation = observe(&go).await;
        assert_eq!(observation, go_observation);
        println!("TASK080_DIFFERENTIAL={go_observation}");
    }
    let persisted = store
        .read(Read::prefix(
            b"/pd/7801/service_middleware",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(persisted.entries.len(), 1);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&persisted.entries[0].value).unwrap(),
        observation["config"]
    );
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(store, cfg, listener, Arc::new(prometheus::Registry::new()))
        .await
        .unwrap();
    node.ready().await.unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            &call(&url, "GET", "service-middleware/config", "").1
        )
        .unwrap(),
        observation["config"]
    );
    let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(url.clone())
        .await
        .unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(2);
    tx.send(astersql_pd_proto::pdpb::QueryRegionRequest {
        header: Some(astersql_pd_proto::pdpb::RequestHeader {
            cluster_id: 7801,
            ..Default::default()
        }),
        ..Default::default()
    })
    .await
    .unwrap();
    let mut held = client
        .query_region(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    held.message().await.unwrap().unwrap();
    let rejected = client
        .query_region(tokio_stream::iter([
            astersql_pd_proto::pdpb::QueryRegionRequest::default(),
        ]))
        .await;
    let error = match rejected {
        Err(e) => e,
        Ok(r) => r.into_inner().message().await.unwrap_err(),
    };
    assert_eq!(error.code(), tonic::Code::ResourceExhausted);
    drop(held);
    drop(tx);
    node.shutdown().await.unwrap();
}

#[test]
fn task_080_pinned_labels() {
    use prost::Message;
    let set =
        prost_types::FileDescriptorSet::decode(astersql_pd_proto::FILE_DESCRIPTOR_SET).unwrap();
    let service = set
        .file
        .iter()
        .filter(|f| f.package.as_deref() == Some("pdpb"))
        .flat_map(|f| &f.service)
        .find(|s| s.name.as_deref() == Some("PD"))
        .unwrap();
    assert_eq!(service.method.len(), 56);
    for method in &service.method {
        assert!(crate::grpc_service_limit::known_label(
            method.name.as_deref().unwrap()
        ));
    }
    assert!(!crate::grpc_service_limit::known_label("GetHealthStatus"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_080_transaction_failure() {
    use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
    use std::sync::atomic::{AtomicU8, Ordering};
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(crate::http_service_limit_test::LimitStore {
        inner: Arc::new(fixture.store().await),
        mode: AtomicU8::new(0),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 7802,
                member_id: 1,
                name: "grpc-fault".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    let mut client = PdClient::connect(url.clone()).await.unwrap();
    let rate = "service-middleware/config/grpc-rate-limit";
    assert_eq!(
        call(&url, "POST", rate, r#"{"label":"GetMembers","qps":0.0001}"#).0,
        200
    );
    client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap();
    let before = call(&url, "GET", "service-middleware/config", "");
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        call(&url, "POST", rate, r#"{"label":"GetMembers","qps":0}"#).0,
        500
    );
    assert_eq!(call(&url, "GET", "service-middleware/config", ""), before);
    // Failed deletion recreates the old bucket with a fresh token, as in Go.
    client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap();
    assert_eq!(
        client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::ResourceExhausted
    );
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        call(
            &url,
            "POST",
            "service-middleware/config",
            r#"{"enable-grpc-rate-limit":"false"}"#
        )
        .0,
        500
    );
    assert_eq!(call(&url, "GET", "service-middleware/config", ""), before);
    assert_eq!(
        client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::ResourceExhausted
    );
    // A newly added live limiter survives a failed persistence transaction.
    assert_eq!(
        call(&url, "POST", rate, r#"{"label":"GetMembers","qps":0}"#).0,
        200
    );
    let before = call(&url, "GET", "service-middleware/config", "");
    store.mode.store(1, Ordering::SeqCst);
    assert_eq!(
        call(&url, "POST", rate, r#"{"label":"GetMembers","qps":0.0001}"#).0,
        500
    );
    assert_eq!(call(&url, "GET", "service-middleware/config", ""), before);
    client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap();
    assert_eq!(
        client
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::ResourceExhausted
    );
    node.shutdown().await.unwrap();
}

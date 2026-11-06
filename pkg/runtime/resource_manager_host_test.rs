// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_keypath::keyspace_resource_group_state_path;
use astersql_pd_proto::resource_manager::{
    self as p, resource_manager_client::ResourceManagerClient,
};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_138_resource_groups_are_term_owned_and_periodically_durable() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let parent = CancellationToken::new();
    let mut nodes = Vec::new();
    let mut urls = Vec::new();
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), parent.child_token())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 13801,
                    member_id: id,
                    name: format!("rm-{id}"),
                    client_url: url.clone(),
                    peer_url: format!("http://peer-{id}"),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        urls.push(url);
    }
    // Reserved groups must already exist when readiness succeeds, before the first RM RPC.
    let state = store
        .read(Read::range(
            keyspace_resource_group_state_path(u32::MAX, "default").as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(state.entries.len(), 1);
    let mut first = ResourceManagerClient::connect(urls[0].clone())
        .await
        .unwrap();
    let mut second = ResourceManagerClient::connect(urls[1].clone())
        .await
        .unwrap();
    assert_eq!(
        second
            .list_resource_groups(p::ListResourceGroupsRequest::default())
            .await
            .unwrap_err()
            .code(),
        tonic::Code::Unavailable
    );
    first
        .add_resource_group(p::PutResourceGroupRequest {
            group: Some(p::ResourceGroup {
                name: "durable".into(),
                mode: 1,
                priority: 8,
                r_u_settings: Some(p::GroupRequestUnitSettings {
                    r_u: Some(p::TokenBucket {
                        tokens: 200000.0,
                        settings: Some(p::TokenLimitSettings {
                            fill_rate: 1000,
                            burst_limit: 20000000,
                            max_tokens: 0.0,
                        }),
                    }),
                }),
                ..Default::default()
            }),
        })
        .await
        .unwrap();
    let (send, recv) = tokio::sync::mpsc::channel(1);
    send.send(p::TokenBucketsRequest {
        requests: vec![p::TokenBucketRequest {
            resource_group_name: "durable".into(),
            request: Some(p::token_bucket_request::Request::RuItems(
                p::token_bucket_request::RequestRu {
                    request_r_u: vec![p::RequestUnitItem {
                        r#type: 0,
                        value: 10000.0,
                    }],
                },
            )),
            ..Default::default()
        }],
        target_request_period_ms: 5000,
        client_unique_id: 5,
    })
    .await
    .unwrap();
    let mut stream = first
        .acquire_token_buckets(tokio_stream::wrappers::ReceiverStream::new(recv))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        stream.message().await.unwrap().unwrap().responses[0].granted_r_u_tokens[0]
            .granted_tokens
            .as_ref()
            .unwrap()
            .tokens,
        10000.0
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(75);
    loop {
        let state = store
            .read(Read::range(
                keyspace_resource_group_state_path(u32::MAX, "durable").as_bytes(),
                b"",
                Consistency::Linearizable,
            ))
            .await
            .unwrap();
        let state: serde_json::Value = serde_json::from_slice(&state.entries[0].value).unwrap();
        if state["r_u"]["tokens"] == 190000 && state["r_u"]["initialized"] == true {
            assert!(state["r_u"]["last_update"].is_string());
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "periodic RM state was not saved: {state}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    nodes[0].shutdown().await.unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .unwrap();
    assert!(
        closed.is_err() || closed.unwrap().is_none(),
        "retired host must stop its token stream"
    );
    drop(send);
    let response = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match second
                .get_resource_group(p::GetResourceGroupRequest {
                    resource_group_name: "durable".into(),
                    ..Default::default()
                })
                .await
            {
                Ok(response) => break response.into_inner(),
                Err(error) => assert_eq!(error.code(), tonic::Code::Unavailable),
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        response
            .group
            .unwrap()
            .r_u_settings
            .unwrap()
            .r_u
            .unwrap()
            .tokens,
        190000.0
    );
    nodes[1].shutdown().await.unwrap();
    assert!(!parent.is_cancelled());
    assert_eq!(tokio::spawn(async { 42 }).await.unwrap(), 42);
}

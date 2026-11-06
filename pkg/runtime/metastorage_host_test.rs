// Copyright 2026 AsterSQL.
use crate::{NodeConfig, PdNodeBuilder};
use astersql_pd_proto::meta_storagepb::{self as p, meta_storage_client::MetaStorageClient};
use astersql_pd_storage_domain::{Consistency, MetaStore, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcs_068_shared_host_state_and_stream_shutdown() {
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
                    cluster_id: 6801,
                    member_id: id,
                    name: format!("meta-{id}"),
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
    let mut first = MetaStorageClient::connect(urls[0].clone()).await.unwrap();
    let response = first
        .put(p::PutRequest {
            key: b"host-meta".to_vec(),
            value: b"durable".to_vec(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let revision = response.header.unwrap().revision;
    let state = store
        .read(Read::range(b"host-meta", b"", Consistency::Linearizable))
        .await
        .unwrap();
    assert_eq!(state.entries[0].value, b"durable");
    assert_eq!(state.entries[0].mod_revision, revision);
    let mut stream = first
        .watch(p::WatchRequest {
            key: b"host-meta".to_vec(),
            start_revision: revision,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        stream.message().await.unwrap().unwrap().events[0]
            .kv
            .as_ref()
            .unwrap()
            .value,
        b"durable"
    );
    let mut second = MetaStorageClient::connect(urls[1].clone()).await.unwrap();
    assert_eq!(
        second
            .get(p::GetRequest {
                key: b"host-meta".to_vec(),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .message(),
        "not leader"
    );
    nodes[0].shutdown().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), stream.message())
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    let recovered = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            match second
                .get(p::GetRequest {
                    key: b"host-meta".to_vec(),
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
    assert_eq!(recovered.kvs[0].value, b"durable");
    assert!(!parent.is_cancelled());
    nodes[1].shutdown().await.unwrap();
    assert_eq!(
        tokio::spawn(async { 41 + 1 }).await.unwrap(),
        42,
        "host runtime remains alive"
    );
}

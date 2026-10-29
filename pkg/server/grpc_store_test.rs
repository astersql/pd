// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_022_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 2200,
                member_id: 1,
                name: "bootstrap".into(),
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
    let observation = super::grpc_store_scenarios_test::observe(&mut client, 2200).await;
    println!("Store RPC observation: {observation}");
    let mut writes = Vec::new();
    for id in 20..32 {
        let mut client = client.clone();
        writes.push(tokio::spawn(async move {
            super::grpc_store_scenarios_test::put(
                &mut client,
                2200,
                super::grpc_store_scenarios_test::meta(id, "one-owner"),
            )
            .await
            .header
            .unwrap()
            .error
            .is_none()
        }));
    }
    let mut successes = 0;
    for write in writes {
        successes += usize::from(write.await.unwrap());
    }
    assert_eq!(
        successes, 1,
        "address validation and persistence must be serialized"
    );
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    use prost::Message;
    let saved = store
        .read(Read::prefix(b"/pd/2200/raft/s/", Consistency::Linearizable))
        .await
        .unwrap();
    let actual = super::grpc_store_scenarios_test::all(&mut client, 2200, false).await;
    let mut durable: Vec<_> = saved
        .entries
        .iter()
        .map(|e| metapb::Store::decode(e.value.as_slice()).unwrap())
        .collect();
    durable.sort_by_key(|s| s.id);
    assert_eq!(actual, durable);
    let heartbeat = client
        .store_heartbeat(pdpb::StoreHeartbeatRequest {
            header: super::grpc_store_scenarios_test::header(2200),
            stats: Some(pdpb::StoreStats {
                store_id: 4,
                capacity: 100,
                available: 80,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(heartbeat.header.unwrap().error.is_none());
    let reply = super::grpc_store_scenarios_test::get(&mut client, 2200, 4).await;
    assert_eq!(reply.stats.unwrap().capacity, 100);
    let ping = std::process::Command::new("curl")
        .args(["-fsS", &format!("{url}/pd/api/v1/ping")])
        .output()
        .unwrap();
    assert!(ping.status.success());
    node.shutdown().await.unwrap();
    let listener = tokio::net::TcpListener::bind(url.trim_start_matches("http://"))
        .await
        .unwrap();
    let mut restarted =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store,
                NodeConfig {
                    cluster_id: 2200,
                    member_id: 1,
                    name: "bootstrap".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    restarted.ready().await.unwrap();
    let mut client = PdClient::connect(url).await.unwrap();
    assert_eq!(
        super::grpc_store_scenarios_test::all(&mut client, 2200, false).await,
        durable
    );
    restarted.shutdown().await.unwrap();
}

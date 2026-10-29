// Copyright 2026 AsterSQL.
use astersql_pd_member::{Leadership, Member};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_server::{Identity, PdServices};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use astersql_pd_tso::{Clock, MAX_LOGICAL, Oracle, Timestamp, TsoError};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;
struct ManualClock(AtomicU64);
impl Clock for ManualClock {
    fn now_millis(&self) -> Result<u64, TsoError> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
struct Host {
    identity: Identity,
    oracle: Oracle,
    leader: Arc<Leadership>,
}
#[async_trait::async_trait]
impl PdServices for Host {
    fn identity(&self) -> &Identity {
        &self.identity
    }
    async fn healthy(&self) -> Result<(), String> {
        self.leader
            .validate()
            .await
            .map_err(|_| "not leader".into())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("not used by this TSO host".into())
    }
    async fn timestamp(&self, count: u32) -> Result<Timestamp, String> {
        crate::tso::allocate(&self.oracle, &self.leader, count).await
    }
}
fn header() -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: 3602,
        ..Default::default()
    })
}
async fn min(client: &mut PdClient<tonic::transport::Channel>) -> pdpb::Timestamp {
    let response = client
        .get_min_ts(pdpb::GetMinTsRequest { header: header() })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    response.timestamp.unwrap()
}
async fn batch(
    client: &mut PdClient<tonic::transport::Channel>,
    count: u32,
) -> Result<pdpb::Timestamp, tonic::Status> {
    let mut stream = client
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header: header(),
            count,
            ..Default::default()
        }]))
        .await?
        .into_inner();
    Ok(stream.message().await?.unwrap().timestamp.unwrap())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_036_clock_backpressure_fencing_and_restart() {
    let fixture = crate::node_test::fixture::Fixture::start();
    let store: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let member = Member::bootstrap(
        store.clone(),
        cancel.clone(),
        3602,
        1,
        b"tso-clock".to_vec(),
    )
    .await
    .unwrap();
    let leader = Arc::new(member.campaign(60).await.unwrap());
    let clock = Arc::new(ManualClock(AtomicU64::new(1000)));
    let oracle = Oracle::new(
        store.clone(),
        leader.clone(),
        clock.clone(),
        cancel.clone(),
        3,
    )
    .await
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let host = Arc::new(Host {
        identity: Identity {
            cluster_id: 3602,
            member_id: 1,
            name: "tso-clock".into(),
            client_url: url.clone(),
            peer_url: "http://peer".into(),
        },
        oracle,
        leader: leader.clone(),
    });
    let router = astersql_pd_server::router(host.clone(), cancel.clone());
    let shutdown = cancel.clone();
    let serving = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap()
    });
    let channel = tonic::transport::Endpoint::from_shared(url)
        .unwrap()
        .initial_stream_window_size(65_535)
        .connect()
        .await
        .unwrap();
    let mut client = PdClient::new(channel);
    assert_eq!(min(&mut client).await.logical, 1);
    assert_eq!(
        batch(&mut client, MAX_LOGICAL - 2).await.unwrap().logical,
        i64::from(MAX_LOGICAL - 1)
    );
    let overflow = batch(&mut client, 2).await.unwrap();
    assert_eq!((overflow.physical, overflow.logical), (1001, 2));
    // Refill a three millisecond durable window via the production host helper.
    let refill = batch(&mut client, MAX_LOGICAL - 1).await.unwrap();
    assert_eq!(
        (refill.physical, refill.logical),
        (1002, i64::from(MAX_LOGICAL - 1))
    );
    clock.0.store(500, Ordering::SeqCst);
    assert_eq!(min(&mut client).await.physical, 1003);
    let key = b"/pd/3602/timestamp";
    let saved = store
        .read(Read::prefix(key, Consistency::Linearizable))
        .await
        .unwrap()
        .entries[0]
        .value
        .clone();
    assert!(u64::from_be_bytes(saved.clone().try_into().unwrap()) > 1003_000_000);
    // Actual compare failure at the etcd transaction boundary, after admission.
    store
        .transact(
            vec![],
            vec![Mutation::put(key, &9000_000_000u64.to_be_bytes())],
        )
        .await
        .unwrap();
    assert_eq!(min(&mut client).await, pdpb::Timestamp::default());
    let error = batch(&mut client, 1).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unknown);
    assert!(error.message().contains("timestamp window replaced"));
    store
        .transact(vec![], vec![Mutation::put(key, &saved)])
        .await
        .unwrap();
    // A peer that stops reading must eventually stall request consumption.
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let mut stream = client
        .tso(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    let producer = tokio::spawn(async move {
        loop {
            if tx
                .send(pdpb::TsoRequest {
                    header: header(),
                    count: 1,
                    ..Default::default()
                })
                .await
                .is_err()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });
    let mut stable = false;
    for _ in 0..200 {
        let a = min(&mut client).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        let b = min(&mut client).await;
        if a.physical == b.physical && b.logical == a.logical + 1 {
            stable = true;
            break;
        }
    }
    assert!(
        stable,
        "unread response stream must stop advancing the oracle"
    );
    if producer.is_finished() {
        let mut received = 0;
        loop {
            match stream.message().await {
                Ok(Some(_)) => received += 1,
                terminal => panic!("producer ended after {received} responses: {terminal:?}"),
            }
        }
    }
    drop(stream);
    producer.abort();
    let _ = producer.await;
    let last = min(&mut client).await;
    // Recreating the same term abandons the old durable window; no overlaps.
    let restarted = Oracle::new(store.clone(), leader.clone(), clock, cancel.clone(), 3)
        .await
        .unwrap();
    let next = restarted.allocate(1).await.unwrap();
    assert!(next.physical > last.physical as u64);
    leader.resign().await.unwrap();
    let error = batch(&mut client, 1).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unknown);
    assert_eq!(
        error.message(),
        "[PD:tso:ErrGenerateTimestamp]generate timestamp failed, requested pd is not leader of cluster"
    );
    let error = client
        .get_min_ts(pdpb::GetMinTsRequest { header: header() })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    let (_tx, rx) = tokio::sync::mpsc::channel(1);
    let mut idle = client
        .tso(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
        .unwrap()
        .into_inner();
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(2), idle.message())
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    drop(idle);
    drop(client);
    tokio::time::timeout(Duration::from_secs(3), serving)
        .await
        .unwrap()
        .unwrap();
}

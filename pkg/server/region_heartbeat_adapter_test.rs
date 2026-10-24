// Copyright 2026 AsterSQL.
use super::region_heartbeat_adapter::*;
use astersql_pd_core::{BasicCluster, NodeState, RegionInfo};
use astersql_pd_proto::{metapb, pdpb};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio_util::sync::CancellationToken;

struct Host {
    boot: AtomicBool,
    enabled: AtomicBool,
    block: AtomicBool,
    active: AtomicUsize,
    identity: crate::Identity,
    cluster: Mutex<BasicCluster>,
    reports: Mutex<Vec<RegionInfo>>,
}
#[async_trait::async_trait]
impl RegionHeartbeatService for Host {
    fn is_bootstrapped(&self) -> bool {
        self.boot.load(Ordering::SeqCst)
    }
    fn region_store_state(&self, id: u64) -> Option<NodeState> {
        match id {
            1 => Some(NodeState::Serving),
            3 => Some(NodeState::Removed),
            _ => None,
        }
    }
    async fn region_heartbeat(&self, region: RegionInfo) -> Result<(), RegionHeartbeatError> {
        if self.block.load(Ordering::SeqCst) {
            struct Active<'a>(&'a AtomicUsize);
            impl Drop for Active<'_> {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::SeqCst);
                }
            }
            self.active.fetch_add(1, Ordering::SeqCst);
            let _active = Active(&self.active);
            std::future::pending::<()>().await;
        }
        self.reports.lock().unwrap().push(region.clone());
        astersql_pd_cluster::handle_region_heartbeat(
            &mut self.cluster.lock().unwrap(),
            region,
            false,
        )
        .map(|_| ())
        .map_err(|e| RegionHeartbeatError(e.to_string()))
    }
}
#[async_trait::async_trait]
impl crate::PdServices for Host {
    fn identity(&self) -> &crate::Identity {
        &self.identity
    }
    fn region_heartbeat_service(&self) -> Option<&dyn RegionHeartbeatService> {
        self.enabled.load(Ordering::SeqCst).then_some(self)
    }
    async fn healthy(&self) -> Result<(), String> {
        Ok(())
    }
    async fn alloc_id(&self) -> Result<u64, String> {
        Err("unused".into())
    }
    async fn timestamp(&self, _: u32) -> Result<astersql_pd_tso::Timestamp, String> {
        Err("unused".into())
    }
}
fn request() -> pdpb::RegionHeartbeatRequest {
    let peer = metapb::Peer {
        id: 2,
        store_id: 1,
        ..Default::default()
    };
    pdpb::RegionHeartbeatRequest {
        header: Some(pdpb::RequestHeader {
            cluster_id: 42,
            ..Default::default()
        }),
        region: Some(metapb::Region {
            id: 1,
            region_epoch: Some(metapb::RegionEpoch {
                conf_ver: 2,
                version: 2,
            }),
            peers: vec![peer.clone()],
            ..Default::default()
        }),
        leader: Some(peer),
        term: 7,
        interval: Some(pdpb::TimeInterval {
            start_timestamp: 10,
            end_timestamp: 20,
        }),
        query_stats: Some(pdpb::QueryStats {
            get: 11,
            put: 13,
            gc: 999,
            ..Default::default()
        }),
        cpu_stats: Some(pdpb::CpuStats {
            unified_read: 17,
            scheduler: 999,
        }),
        approximate_size: 2097152,
        ..Default::default()
    }
}
async fn setup() -> (
    Arc<Host>,
    pdpb::pd_client::PdClient<tonic::transport::Channel>,
    CancellationToken,
    tokio::task::JoinHandle<()>,
) {
    let host = Arc::new(Host {
        boot: AtomicBool::new(true),
        enabled: AtomicBool::new(true),
        block: AtomicBool::new(false),
        active: AtomicUsize::new(0),
        identity: crate::Identity {
            cluster_id: 42,
            member_id: 1,
            name: "state48".into(),
            client_url: String::new(),
            peer_url: String::new(),
        },
        cluster: Mutex::new(BasicCluster::new()),
        reports: Mutex::new(vec![]),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let router = crate::router(host.clone(), cancel.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    let client = pdpb::pd_client::PdClient::connect(url).await.unwrap();
    (host, client, cancel, server)
}
#[tokio::test]
async fn state_task_48_region_heartbeat_adapter_loopback() {
    let (host, mut client, cancel, server) = setup().await;
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let base = request();
    tx.send(base.clone()).await.unwrap();
    tx.send(base.clone()).await.unwrap();
    let mut stale = base;
    stale
        .region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .version = 1;
    tx.send(stale).await.unwrap();
    drop(tx);
    let response = client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await;
    let result = match response {
        Ok(r) => r.into_inner().message().await,
        Err(e) => Err(e),
    };
    drop(client);
    cancel.cancel();
    server.await.unwrap();
    let response = result.unwrap().unwrap();
    assert_eq!(
        response.header.unwrap().error.unwrap().r#type,
        pdpb::ErrorType::Unknown as i32
    );
    assert_eq!(response.region_id, 0);
    assert!(response.region_epoch.is_none());
    assert_eq!(response.target_peer.unwrap().id, 2);
    let reports = host.reports.lock().unwrap();
    assert_eq!(reports.len(), 3);
    let report = &reports[0];
    assert_eq!(report.term(), 7);
    assert_eq!(report.report_interval(), (10, 20));
    assert_eq!(report.read_query_num(), 11);
    assert_eq!(report.write_query_num(), 13);
    assert_eq!(report.loads()[6], 17.);
    assert_eq!(report.meta().epoch.unwrap().conf_ver, 2);
    assert_eq!(
        host.cluster
            .lock()
            .unwrap()
            .regions
            .get_region(1)
            .unwrap()
            .meta()
            .epoch
            .unwrap()
            .version,
        2
    );
}

async fn exchange(
    client: &mut pdpb::pd_client::PdClient<tonic::transport::Channel>,
    requests: Vec<pdpb::RegionHeartbeatRequest>,
) -> Result<Option<pdpb::RegionHeartbeatResponse>, tonic::Status> {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    for r in requests {
        tx.send(r).await.unwrap();
    }
    drop(tx);
    match client
        .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(rx))
        .await
    {
        Ok(r) => r.into_inner().message().await,
        Err(e) => Err(e),
    }
}
#[tokio::test]
async fn state_task_48_region_heartbeat_adapter_errors_and_close() {
    let (host, mut client, cancel, server) = setup().await;
    let mut outcomes = vec![];
    let mut nil = request();
    nil.leader = None;
    outcomes.push(exchange(&mut client, vec![nil]).await);
    let mut bad = request();
    bad.header = None;
    outcomes.push(exchange(&mut client, vec![bad]).await);
    for kind in 0..4 {
        let mut bad = request();
        match kind {
            0 => bad.region.as_mut().unwrap().id = 0,
            1 => bad.region.as_mut().unwrap().peers.clear(),
            2 => bad.region = None,
            _ => bad.down_peers.push(pdpb::PeerStats {
                peer: None,
                down_seconds: 1,
            }),
        }
        outcomes.push(exchange(&mut client, vec![bad]).await);
    }
    let mut removed = request();
    removed.leader.as_mut().unwrap().store_id = 3;
    let removed = exchange(&mut client, vec![removed]).await;
    let empty = exchange(&mut client, vec![]).await;
    host.boot.store(false, Ordering::SeqCst);
    let bootstrap = exchange(&mut client, vec![request()]).await;
    host.enabled.store(false, Ordering::SeqCst);
    let absent = exchange(&mut client, vec![request()]).await;
    drop(client);
    cancel.cancel();
    server.await.unwrap();
    assert_eq!(
        outcomes.remove(0).unwrap_err().message(),
        "invalid store ID 0, not found"
    );
    assert_eq!(
        outcomes.remove(0).unwrap_err().code(),
        tonic::Code::FailedPrecondition
    );
    for result in outcomes {
        let response = result.unwrap().unwrap();
        assert_eq!(
            response.header.unwrap().error.unwrap().r#type,
            pdpb::ErrorType::Unknown as i32
        );
        assert_eq!(response.target_peer.unwrap().id, 2);
    }
    assert!(removed.unwrap().is_none());
    assert!(empty.unwrap().is_none());
    assert_eq!(
        bootstrap
            .unwrap()
            .unwrap()
            .header
            .unwrap()
            .error
            .unwrap()
            .r#type,
        pdpb::ErrorType::NotBootstrapped as i32
    );
    assert_eq!(absent.unwrap_err().code(), tonic::Code::Unimplemented);
    assert!(host.reports.lock().unwrap().is_empty());
    assert!(host.cluster.lock().unwrap().regions.get_region(1).is_none());
}
#[tokio::test]
async fn state_task_48_region_heartbeat_adapter_defaults_and_epoch() {
    let (host, mut client, cancel, server) = setup().await;
    let mut defaults = request();
    defaults.interval = None;
    defaults.query_stats = None;
    defaults.cpu_stats = None;
    let accepted = exchange(&mut client, vec![defaults.clone()]).await;
    let mut conf = defaults.clone();
    conf.region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .conf_ver = 1;
    let stale_conf = exchange(&mut client, vec![conf]).await;
    let mut term = defaults.clone();
    term.term = 6;
    let stale_term = exchange(&mut client, vec![term]).await;
    let mut next = defaults;
    next.region
        .as_mut()
        .unwrap()
        .region_epoch
        .as_mut()
        .unwrap()
        .conf_ver = 3;
    next.term = 8;
    let updated = exchange(&mut client, vec![next]).await;
    drop(client);
    cancel.cancel();
    server.await.unwrap();
    assert!(accepted.unwrap().is_none());
    assert!(updated.unwrap().is_none());
    for result in [stale_conf, stale_term] {
        assert!(
            result
                .unwrap()
                .unwrap()
                .header
                .unwrap()
                .error
                .unwrap()
                .message
                .starts_with("region is stale")
        );
    }
    let cluster = host.cluster.lock().unwrap();
    let region = cluster.regions.get_region(1).unwrap();
    assert_eq!(region.term(), 8);
    assert_eq!(region.meta().epoch.unwrap().conf_ver, 3);
    assert_eq!(region.report_interval(), (0, 0));
    assert_eq!(region.loads()[6], 0.);
    assert_eq!(region.read_query_num(), 0);
}
#[tokio::test]
async fn state_task_48_region_heartbeat_adapter_cancel_inflight() {
    let (host, mut client, cancel, server) = setup().await;
    host.block.store(true, Ordering::SeqCst);
    let call = tokio::spawn(async move { exchange(&mut client, vec![request()]).await });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while host.active.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), call)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    assert!(result.unwrap().is_none());
    assert_eq!(host.active.load(Ordering::SeqCst), 0);
    assert!(host.reports.lock().unwrap().is_empty());
}

#[tokio::test]
async fn state_task_48_region_heartbeat_adapter_disconnect() {
    let (host, mut client, cancel, server) = setup().await;
    host.block.store(true, Ordering::SeqCst);
    let call = tokio::spawn(async move { exchange(&mut client, vec![request()]).await });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while host.active.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    call.abort();
    let _ = call.await;
    let released = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while host.active.load(Ordering::SeqCst) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    cancel.cancel();
    server.await.unwrap();
    assert!(released.is_ok());
    assert!(host.reports.lock().unwrap().is_empty());
}

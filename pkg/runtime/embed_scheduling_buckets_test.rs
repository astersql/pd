// Copyright 2026 AsterSQL.
//! Actual MCS wire admission and the same volatile domain state in an embedded host.
use super::*;
use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
use astersql_pd_proto::{metapb, pdpb, schedulingpb as p};
use serde_json::json;
struct Drain(tokio::task::JoinHandle<()>);
impl Drop for Drain {
    fn drop(&mut self) {
        self.0.abort();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_embedded_scheduling_buckets_publish_shared_volatile_state() {
    let mut pd = Server::start(&pinned_go_binary().unwrap()).unwrap();
    pd.ready().unwrap();
    let mut owner = pdpb::pd_client::PdClient::connect(pd.url.clone())
        .await
        .unwrap();
    let cluster = owner
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let mut region = metapb::Region {
        id: 2,
        region_epoch: Some(metapb::RegionEpoch {
            version: 1,
            conf_ver: 1,
        }),
        peers: vec![peer.clone()],
        ..Default::default()
    };
    assert!(
        owner
            .bootstrap(pdpb::BootstrapRequest {
                header,
                store: Some(metapb::Store {
                    id: 1,
                    address: "embedded-bucket-store".into(),
                    node_state: metapb::NodeState::Serving as i32,
                    ..Default::default()
                }),
                region: Some(region.clone())
            })
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .error
            .is_none()
    );
    let backend = Arc::new(
        astersql_pd_storage::EtcdMetaStore::connect(
            &[pd.metadata_url().into()],
            Duration::from_secs(2),
        )
        .await
        .unwrap(),
    );
    let key = ClusterPaths::new(cluster).region_path(2);
    let before = backend
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap();
    let parent = CancellationToken::new();
    let _stop = parent.clone().drop_guard();
    let runtime = Handle::current();
    let delegates = Arc::new(DelegateConnections::new(runtime.clone(), &parent, None));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let lookup = backend.clone();
    let host = SchedulingHost::start(
        SchedulingHostConfig {
            runtime: runtime.clone(),
            cancel: parent.clone(),
            store: backend.clone(),
            cluster_id: cluster,
            entry: ServiceRegistryEntry {
                name: "embedded-buckets".into(),
                service_addr: address.clone(),
                ..Default::default()
            },
            delegates: delegates.clone(),
            registry: Arc::new(prometheus::Registry::new()),
            pd_leader_lookup: Arc::new(move || {
                let backend = lookup.clone();
                Box::pin(async move {
                    backend
                        .consensus_leader_client_url()
                        .await
                        .map_err(|e| e.to_string())
                })
            }),
        },
        listener,
    )
    .await
    .unwrap();
    let domain = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let term = host.state.term.read().unwrap().clone();
            if let Some(term) = term {
                break term.cluster.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut client = p::scheduling_client::SchedulingClient::connect(address)
        .await
        .unwrap();
    let header = Some(p::RequestHeader {
        cluster_id: cluster,
        sender_id: 0,
    });
    let response = client
        .store_heartbeat(p::StoreHeartbeatRequest {
            header: header.clone(),
            stats: Some(pdpb::StoreStats {
                store_id: 1,
                capacity: 100 * 1024 * 1024 * 1024,
                available: 90 * 1024 * 1024 * 1024,
                region_count: 1,
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response.header.unwrap().error.is_none());
    region.start_key = b"a".to_vec();
    region.end_key = b"b".to_vec();
    region.region_epoch.as_mut().unwrap().version = 2;
    let (send, receive) = tokio::sync::mpsc::channel(4);
    let mut heartbeat = client.clone();
    let _drain = Drain(runtime.spawn(async move {
        if let Ok(response) = heartbeat
            .region_heartbeat(tokio_stream::wrappers::ReceiverStream::new(receive))
            .await
        {
            let mut stream = response.into_inner();
            while stream.message().await.is_ok_and(|m| m.is_some()) {}
        }
    }));
    send.send(p::RegionHeartbeatRequest {
        header: header.clone(),
        region: Some(region.clone()),
        leader: Some(peer),
        approximate_size: 2 * 1024 * 1024,
        approximate_keys: 1,
        ..Default::default()
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while domain
            .snapshot()
            .await
            .unwrap()
            .regions
            .iter()
            .all(|r| r.meta().id != 2)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let mut records = vec![];
    for (version, middle, expected) in [
        (3, b"a1".to_vec(), b"a1".to_vec()),
        (2, b"stale".to_vec(), b"a1".to_vec()),
        (3, b"equal".to_vec(), b"a1".to_vec()),
        (4, b"a2".to_vec(), b"a2".to_vec()),
    ] {
        let request = p::RegionBucketsRequest {
            header: header.clone(),
            region_epoch: region.region_epoch.clone(),
            buckets: Some(metapb::Buckets {
                region_id: 2,
                version,
                keys: vec![b"a".to_vec(), middle, b"b".to_vec()],
                stats: Some(metapb::BucketStats {
                    read_bytes: vec![200000, 400000],
                    write_bytes: vec![1000, 2000],
                    read_qps: vec![10, 20],
                    write_qps: vec![1, 2],
                    read_keys: vec![2, 4],
                    write_keys: vec![1, 2],
                }),
                period_in_ms: 2000,
            }),
        };
        let mut output = client
            .region_buckets(tokio_stream::iter([request]))
            .await
            .unwrap()
            .into_inner();
        assert!(
            output
                .message()
                .await
                .unwrap()
                .unwrap()
                .header
                .unwrap()
                .error
                .is_none()
        );
        assert!(output.message().await.unwrap().is_none());
        let observed=tokio::time::timeout(Duration::from_secs(5),async {
            loop {
                let snapshot=domain.snapshot().await.unwrap();let report=snapshot.regions.iter().find(|r|r.meta().id==2).unwrap().report_buckets();
                if let Some(report)=report {
                    if report.version==if version==4 {4}else{3} && report.keys[1]==expected && snapshot.statistics.buckets.get(&2).is_some_and(|v|!v.is_empty()) {break json!({"submitted_version":version,"observed_version":report.version,"keys":report.keys,"read_bytes":report.stats.as_ref().unwrap().read_bytes,"hot_buckets_present":true});}
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        records.push(observed);
        assert_eq!(
            backend
                .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                .await
                .unwrap()
                .entries,
            before.entries
        );
    }
    drop(send);
    drop(client);
    host.shutdown().await.unwrap();
    assert!(!parent.is_cancelled());
    assert_eq!(runtime.spawn(async { 7 }).await.unwrap(), 7);
    assert!(
        owner
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .is_ok()
    );
    assert_eq!(
        backend
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries,
        before.entries
    );
    let path = std::path::PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("embedded-buckets.json");
    std::fs::write(path,serde_json::to_vec(&json!({"observations":records,"pd_region_unchanged":true,"parent_runtime_alive":true,"boundary":"real Go PD metadata owner and genuine elected Rust SchedulingHost, typed MCS heartbeat/buckets clients; same canonical cache and HotStat observed directly, no fake domain host"})).unwrap()).unwrap();
    drop(domain);
    delegates.shutdown();
    drop(delegates);
    drop(owner);
    drop(backend);
    pd.stop().unwrap();
}

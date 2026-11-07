// Copyright 2026 AsterSQL.
//! Native-backend fault evidence for the production Scheduling config consumer.
use super::*;
use astersql_pd_compat_tests::harness::Server;
use astersql_pd_proto::pdpb;
use astersql_pd_storage::*;
use astersql_pd_storage_endpoint::StorageEndpoint;
use serde_json::json;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
};
#[derive(Default)]
struct Observations {
    starts: Mutex<Vec<Revision>>,
    delivered: AtomicI64,
    compacted: AtomicUsize,
    gate: AtomicBool,
    entered: Notify,
    release: Notify,
}
struct RecordedStore {
    inner: Arc<dyn MetaStore>,
    config_key: Vec<u8>,
    observations: Arc<Observations>,
}
struct RecordedWatch {
    inner: Box<dyn MetaWatch>,
    observations: Arc<Observations>,
}
#[async_trait::async_trait]
impl MetaWatch for RecordedWatch {
    async fn next(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        let result = self.inner.next(cancel).await;
        match &result {
            Ok(batch) => {
                let six = batch.events.iter().any(|event| match event {
                    Event::Put(row) => serde_json::from_slice::<serde_json::Value>(&row.value)
                        .is_ok_and(|v| v["replication"]["max-replicas"] == 6),
                    _ => false,
                });
                if six && self.observations.gate.swap(false, Ordering::SeqCst) {
                    self.observations.entered.notify_one();
                    tokio::select! {biased;_=cancel.cancelled()=>return Err(StoreError::Cancelled),_=self.observations.release.notified()=>{}}
                }
                self.observations
                    .delivered
                    .fetch_max(batch.revision, Ordering::SeqCst);
            }
            Err(StoreError::Compacted { .. }) => {
                self.observations.compacted.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
        result
    }
}
#[async_trait::async_trait]
impl MetaStore for RecordedStore {
    async fn read(&self, request: Read) -> Result<Snapshot> {
        self.inner.read(request).await
    }
    async fn transact(&self, compares: Vec<Compare>, writes: Vec<Mutation>) -> Result<Commit> {
        self.inner.transact(compares, writes).await
    }
    async fn transact_branches(
        &self,
        compares: Vec<Compare>,
        then_writes: Vec<Mutation>,
        else_writes: Vec<Mutation>,
    ) -> Result<Commit> {
        self.inner
            .transact_branches(compares, then_writes, else_writes)
            .await
    }
    async fn transact_raw(
        &self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        self.inner.transact_raw(compares, then_ops, else_ops).await
    }
    async fn watch(&self, key: Vec<u8>, revision: Revision) -> Result<Box<dyn MetaWatch>> {
        let watch = self.inner.watch(key.clone(), revision).await?;
        if key == self.config_key {
            self.observations.starts.lock().unwrap().push(revision);
            Ok(Box::new(RecordedWatch {
                inner: watch,
                observations: self.observations.clone(),
            }))
        } else {
            Ok(watch)
        }
    }
    async fn grant(&self, ttl_seconds: i64) -> Result<Lease> {
        self.inner.grant(ttl_seconds).await
    }
    async fn keep_alive(&self, lease: LeaseId) -> Result<Lease> {
        self.inner.keep_alive(lease).await
    }
    async fn revoke(&self, lease: LeaseId) -> Result<()> {
        self.inner.revoke(lease).await
    }
    async fn ttl(&self, lease: LeaseId) -> Result<i64> {
        self.inner.ttl(lease).await
    }
    async fn compact(&self, revision: Revision) -> Result<()> {
        self.inner.compact(revision).await
    }
}

async fn configuration(store: &dyn MetaStore, key: &str) -> Snapshot {
    store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await
        .unwrap()
}
async fn produce(store: Arc<dyn MetaStore>, paths: &ClusterPaths, count: u64) -> Snapshot {
    let mut value = PersistedConfig::default();
    value
        .config
        .adjust(
            &Default::default(),
            ConfigDefaultsContext {
                hostname: "native-config",
                cwd: &std::env::current_dir().unwrap(),
            },
            false,
            &mut Default::default(),
        )
        .unwrap();
    value.config.replication.max_replicas = count;
    value.config.replication.enable_placement_rules = false;
    value.config.schedule.halt_scheduling = true;
    value
        .persist(&StorageEndpoint::new(store.clone()), paths)
        .await
        .unwrap();
    configuration(store.as_ref(), &paths.config_path()).await
}
async fn observed(term: &Term, count: u64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if term
                .cluster
                .configuration()
                .await
                .unwrap()
                .replication
                .max_replicas
                == count
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("configuration generation must become visible");
}
async fn delivered(stats: &Observations, revision: Revision) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while stats.delivered.load(Ordering::SeqCst) < revision {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("real native watch must deliver the fault event");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_scheduling_native_config_compaction_invalid_delete_and_cancel() {
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut pd = Server::start_rust(&rust).unwrap();
    pd.ready().unwrap();
    let mut client = pdpb::pd_client::PdClient::connect(pd.url.clone())
        .await
        .unwrap();
    let cluster = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    drop(client);
    let backend = Arc::new(
        astersql_pd_storage::EtcdMetaStore::connect(
            &[pd.metadata_url().into()],
            Duration::from_secs(2),
        )
        .await
        .unwrap(),
    );
    let paths = ClusterPaths::new(cluster);
    let key = paths.config_path();
    // Fault producer writes canonical valid documents, then invalid/deleted input.
    // This supplements the separate public-PD-HTTP Go/Rust live-update test.
    let initial = produce(backend.clone(), &paths, 5).await;
    let observations = Arc::new(Observations::default());
    let store: Arc<dyn MetaStore> = Arc::new(RecordedStore {
        inner: backend.clone(),
        config_key: key.as_bytes().to_vec(),
        observations: observations.clone(),
    });
    let cancel = CancellationToken::new();
    let _cancel_on_exit = cancel.clone().drop_guard();
    let runtime = Handle::current();
    let delegates = Arc::new(DelegateConnections::new(runtime.clone(), &cancel, None));
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", tcp.local_addr().unwrap());
    let lookup_backend = backend.clone();
    let pd_key = paths.election_path(None);
    let config = SchedulingHostConfig {
        runtime: runtime.clone(),
        cancel: cancel.clone(),
        store: store.clone(),
        cluster_id: cluster,
        entry: ServiceRegistryEntry {
            name: "native-config".into(),
            service_addr: address.clone(),
            ..Default::default()
        },
        delegates: delegates.clone(),
        registry: Arc::new(prometheus::Registry::new()),
        pd_leader_lookup: Arc::new(move || {
            let store = lookup_backend.clone();
            let key = pd_key.clone();
            Box::pin(async move {
                let value = store
                    .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
                    .await
                    .map_err(|e| e.to_string())?;
                value
                    .entries
                    .first()
                    .map(|row| {
                        pdpb::Member::decode(row.value.as_slice())
                            .map(|m| m.client_urls.into_iter().next())
                            .map_err(|e| e.to_string())
                    })
                    .transpose()
                    .map(Option::flatten)
            })
        }),
    };
    let participant = Participant {
        name: address.clone(),
        id: astersql_pd_memberutil::generate_unique_id(&address),
        listen_urls: vec![address],
    };
    let param = MsParam {
        service_name: "scheduling".into(),
        group_id: 0,
    };
    let election_key = paths.election_path(Some(&param));
    let leader = Arc::new(
        Leadership::campaign_with_key(
            store.clone(),
            cancel.clone(),
            cluster,
            election_key.as_bytes().to_vec(),
            participant.encode_to_vec(),
            9,
            vec![],
        )
        .await
        .unwrap(),
    );
    let keep = leader.keep(&runtime, cancel.clone());
    let (term, store_watch, config_watch) = open(
        &config,
        leader.clone(),
        cancel.clone(),
        Arc::new(Notify::new()),
    )
    .await
    .unwrap();
    observed(&term, 5).await;
    let cursor = config_watch.next_revision();
    let updated = produce(backend.clone(), &paths, 2).await;
    // Keep the stale cursor strictly older than compaction. The consumer no
    // longer performs shared-storage housekeeping writes to advance revisions.
    let barrier_key = format!("/sa016/config-compaction/{cluster}");
    let barrier = backend
        .transact(
            vec![],
            vec![Mutation::put(barrier_key.as_bytes(), b"barrier")],
        )
        .await
        .unwrap();
    backend.compact(barrier.revision).await.unwrap();
    assert!(cursor < barrier.revision);
    let worker = runtime.spawn(crate::cluster_host::scheduling_config_watch::run(
        term.cluster.clone(),
        config_watch,
        cancel.clone(),
    ));
    observed(&term, 2).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while observations.compacted.load(Ordering::SeqCst) == 0
            || observations.starts.lock().unwrap().len() < 2
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("actual native compacted cursor must be replaced");
    let starts = observations.starts.lock().unwrap().clone();
    assert_eq!(starts[0], cursor);
    assert!(starts[1] > barrier.revision);
    assert_eq!(
        configuration(backend.as_ref(), &key).await.entries,
        updated.entries
    );
    // Hold one real native watch response while a later malformed write lands.
    // The valid event must still become the last valid generation, as Go's
    // putFn decodes event.Kv.Value rather than reading the latest key again.
    observations.gate.store(true, Ordering::SeqCst);
    let valid_six = produce(backend.clone(), &paths, 6).await;
    tokio::time::timeout(Duration::from_secs(5), observations.entered.notified())
        .await
        .unwrap();
    let burst_bad = backend
        .transact(vec![], vec![Mutation::put(key.as_bytes(), b"{")])
        .await
        .unwrap();
    observations.release.notify_one();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let current = term
        .cluster
        .configuration()
        .await
        .unwrap()
        .replication
        .max_replicas;
    let burst_evidence = std::path::PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("native-config-burst.json");
    std::fs::write(burst_evidence,serde_json::to_vec(&json!({"valid_revision":valid_six.revision,"later_invalid_revision":burst_bad.revision,"observed_replicas":current,"expected_last_valid":6,"stale_cursor":cursor,"native_compaction_errors":observations.compacted.load(Ordering::SeqCst),"watch_starts":starts})).unwrap()).unwrap();
    observed(&term, 6).await;
    assert_eq!(
        configuration(backend.as_ref(), &key).await.entries[0].value,
        b"{"
    );
    produce(backend.clone(), &paths, 2).await;
    observed(&term, 2).await;
    let deletion = backend
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: key.as_bytes().to_vec(),
            }],
        )
        .await
        .unwrap();
    delivered(&observations, deletion.revision).await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    observed(&term, 2).await;
    assert!(
        configuration(backend.as_ref(), &key)
            .await
            .entries
            .is_empty()
    );
    let invalid = backend
        .transact(vec![], vec![Mutation::put(key.as_bytes(), b"{")])
        .await
        .unwrap();
    delivered(&observations, invalid.revision).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    observed(&term, 2).await;
    let bad = configuration(backend.as_ref(), &key).await;
    assert_eq!(bad.entries[0].value, b"{");
    let recovered = produce(backend.clone(), &paths, 4).await;
    observed(&term, 4).await;
    assert_eq!(
        configuration(backend.as_ref(), &key).await.entries,
        recovered.entries
    );
    let last_bad = backend
        .transact(vec![], vec![Mutation::put(key.as_bytes(), b"{")])
        .await
        .unwrap();
    delivered(&observations, last_bad.revision).await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(2), worker)
        .await
        .unwrap()
        .unwrap();
    term.cluster.shutdown().await.unwrap();
    keep.join().await.unwrap();
    leader.resign().await.unwrap();
    assert!(
        configuration(backend.as_ref(), &election_key)
            .await
            .entries
            .is_empty()
    );
    assert_eq!(
        configuration(backend.as_ref(), &key).await.entries[0].value,
        b"{"
    );
    backend
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: barrier_key.into_bytes(),
            }],
        )
        .await
        .unwrap();
    let receipt = json!({"backend":pd.metadata_url(),"pd_url":pd.url,"cluster":cluster,"native_primary_lease":leader.lease_id(),"initial_revision":initial.revision,"stale_cursor":cursor,"compacted_revision":barrier.revision,"replacement_cursors":starts,"native_compaction_errors":observations.compacted.load(Ordering::SeqCst),"deletion_revision":deletion.revision,"invalid_revision":invalid.revision,"recovered_revision":recovered.revision,"cancel_invalid_revision":last_bad.revision,"burst_valid_revision":valid_six.revision,"burst_invalid_revision":burst_bad.revision,"observed_replica_generations":[5,2,6,2,2,2,4],"no_writeback":true,"cancel_join_seconds_bound":2,"primary_removed":true,"scope":"real native etcd with actual Rust PD identity and real Scheduling election; canonical private term assembly and config consumer, not full MCS CLI or Go fault differential; producer inputs are direct canonical metadata fault writes"});
    let evidence = std::path::PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("native-config-faults.json");
    std::fs::write(evidence, serde_json::to_vec(&receipt).unwrap()).unwrap();
    drop(store_watch);
    drop(term);
    drop(config);
    drop(tcp);
    delegates.shutdown();
    drop(delegates);
    drop(leader);
    drop(store);
    drop(backend);
    pd.stop().unwrap();
}

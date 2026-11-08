// Copyright 2026 AsterSQL.

use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    path::Path,
    sync::Arc,
    time::Duration,
};

use astersql_pd_keypath::ClusterPaths;
use astersql_pd_runtime::{NodeConfig, PdNode, PdNodeBuilder, bootstrap_config};
use astersql_pd_storage_kv::{
    Compare, Consistency, MetaStore, Mutation, NativeMetaStore, Read, Snapshot, open_isolated,
    restore_native_backup,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::{main_test::fixture, native_process_test_support::observe};

const PROBE: &[u8] = b"/pd/local-cutover/confirmed";
const LEASED: &[u8] = b"/pd/local-cutover/source-lease";

async fn start(store: Arc<dyn MetaStore>, config: &NodeConfig, listener: TcpListener) -> PdNode {
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store,
            config.clone(),
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    node
}

async fn freeze(node: &mut PdNode, address: SocketAddr) {
    node.shutdown().await.unwrap();
    assert!(tokio::net::TcpStream::connect(address).await.is_err());
}

async fn snapshot(store: &dyn MetaStore) -> Snapshot {
    let state = store
        .read(Read::prefix(b"/pd/", Consistency::Linearizable))
        .await
        .unwrap();
    assert!(!state.more);
    state
}

fn durable(state: Snapshot) -> BTreeMap<Vec<u8>, Vec<u8>> {
    state
        .entries
        .into_iter()
        .filter(|e| e.lease.is_none())
        .map(|e| (e.key, e.value))
        .collect()
}

async fn leader(store: &NativeMetaStore) {
    store
        .consensus()
        .wait(Some(Duration::from_secs(5)))
        .current_leader(1, "local cutover voter")
        .await
        .unwrap();
}

async fn stage(state: &Snapshot, staging: &Path) -> Vec<u8> {
    // Isolated test bridge. Copy bytes from a single etcd revision and use the
    // existing native backup format. Backend-specific revisions stay separate.
    let store = open_isolated(staging, 38, 1).await.unwrap();
    store
        .consensus()
        .initialize(BTreeSet::from([1]))
        .await
        .unwrap();
    leader(&store).await;
    let lease = store.grant(60).await.unwrap().id;
    let writes = state
        .entries
        .iter()
        .map(|e| Mutation::Put {
            key: e.key.clone(),
            value: e.value.clone(),
            lease: e.lease.map(|_| lease),
        })
        .collect();
    assert!(store.transact(vec![], writes).await.unwrap().succeeded);
    let archive = store.backup().await.unwrap();
    store.close().await.unwrap();
    archive
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn theme10_native_etcd_cutover_local() {
    let mut fixture = fixture::Fixture::start();
    let source = Arc::new(fixture.store().await);
    let disk = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let config = bootstrap_config(
        source.as_ref(),
        "cutover".into(),
        format!("http://{address}"),
        "http://127.0.0.1:2380".into(),
    )
    .await
    .unwrap();
    let mut source_pd = start(source.clone(), &config, listener).await;
    let first = observe(address).await;
    let lease = source.grant(60).await.unwrap().id;
    assert!(
        source
            .transact(
                vec![],
                vec![
                    Mutation::put(PROBE, b"confirmed on etcd"),
                    Mutation::Put {
                        key: LEASED.to_vec(),
                        value: b"source session".to_vec(),
                        lease: Some(lease)
                    },
                ]
            )
            .await
            .unwrap()
            .succeeded
    );

    freeze(&mut source_pd, address).await;
    let frozen = snapshot(source.as_ref()).await;
    let archive = stage(&frozen, &disk.path().join("staging-rejected")).await;
    let mut corrupt: serde_json::Value = serde_json::from_slice(&archive).unwrap();
    corrupt["image"]["next_lease"] = 99.into();
    let corrupt = serde_json::to_vec(&corrupt).unwrap();
    let rejected = disk.path().join("rejected");
    let error = restore_native_backup(&corrupt, &rejected, 39, 1)
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid native archive format or checksum"
    );
    assert!(!rejected.exists());

    // A direct writer outside the stopped PD listener invalidates the source
    // checkpoint. Keep that acknowledged write when rolling back to source.
    assert!(
        source
            .transact(
                vec![Compare::Value(
                    PROBE.to_vec(),
                    b"confirmed on etcd".to_vec()
                )],
                vec![Mutation::put(PROBE, b"late etcd write")]
            )
            .await
            .unwrap()
            .succeeded
    );
    let advanced = snapshot(source.as_ref()).await;
    assert!(advanced.revision > frozen.revision);
    drop(source_pd);
    drop(source);
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    let source = Arc::new(fixture.store().await);
    let recovered = snapshot(source.as_ref()).await;
    assert_eq!(recovered.revision, advanced.revision);
    assert_eq!(durable(recovered)[PROBE], b"late etcd write");
    let mut rollback = start(
        source.clone(),
        &config,
        TcpListener::bind(address).await.unwrap(),
    )
    .await;
    let returned = observe(address).await;
    assert_eq!(returned.0, first.0);
    assert!(returned.1 > first.1 && returned.2 > first.2);
    freeze(&mut rollback, address).await;

    let frozen = snapshot(source.as_ref()).await;
    assert!(
        frozen
            .entries
            .iter()
            .any(|entry| entry.key == LEASED && entry.lease == Some(lease))
    );
    let archive = stage(&frozen, &disk.path().join("staging")).await;
    let target_dir = disk.path().join("target");
    restore_native_backup(&archive, &target_dir, 39, 1)
        .await
        .unwrap();
    assert_eq!(snapshot(source.as_ref()).await.revision, frozen.revision);
    let target = Arc::new(open_isolated(&target_dir, 39, 1).await.unwrap());
    leader(&target).await;
    assert_eq!(target.consensus_cluster_id().await.unwrap(), Some(39));
    let restored = snapshot(target.as_ref()).await;
    assert!(restored.entries.iter().all(|entry| entry.key != LEASED));
    assert_eq!(durable(restored), durable(frozen.clone()));
    assert!(
        !target
            .transact(
                vec![Compare::Lease(LEASED.to_vec(), lease)],
                vec![Mutation::put(PROBE, b"stale session")]
            )
            .await
            .unwrap()
            .succeeded
    );

    let mut native_pd = start(
        target.clone(),
        &config,
        TcpListener::bind(address).await.unwrap(),
    )
    .await;
    let moved = observe(address).await;
    assert_eq!(moved.0, first.0);
    assert!(moved.1 > returned.1 && moved.2 > returned.2);
    assert!(
        target
            .transact(
                vec![Compare::Value(PROBE.to_vec(), b"late etcd write".to_vec())],
                vec![Mutation::put(PROBE, b"confirmed on native")]
            )
            .await
            .unwrap()
            .succeeded
    );
    freeze(&mut native_pd, address).await;
    drop(native_pd);
    Arc::try_unwrap(target).ok().unwrap().close().await.unwrap();

    let target = open_isolated(&target_dir, 39, 1).await.unwrap();
    leader(&target).await;
    let native = durable(snapshot(&target).await);
    assert_eq!(native[PROBE], b"confirmed on native");
    let still_frozen = snapshot(source.as_ref()).await;
    assert_eq!(still_frozen.revision, frozen.revision);
    let before_replay = durable(still_frozen);
    assert_eq!(before_replay[PROBE], b"late etcd write");
    target.close().await.unwrap();

    // This fixture can replay all of its durable changes in one bounded
    // transaction. Refuse to start the source if any other key changed.
    let paths = ClusterPaths::new(config.cluster_id);
    let alloc = paths.alloc_id_path();
    let tso = paths.timestamp_path(0);
    let changed = BTreeSet::from([PROBE, alloc.as_bytes(), tso.as_bytes()]);
    let observed: BTreeSet<_> = native
        .keys()
        .chain(before_replay.keys())
        .map(Vec::as_slice)
        .filter(|key| native.get(*key) != before_replay.get(*key))
        .collect();
    assert_eq!(observed, changed);
    let comparisons: Vec<_> = frozen
        .entries
        .iter()
        .filter(|entry| changed.contains(entry.key.as_slice()))
        .map(|entry| Compare::ModRevision(entry.key.clone(), entry.mod_revision))
        .collect();
    assert_eq!(comparisons.len(), changed.len());
    let writes: Vec<_> = changed
        .iter()
        .map(|key| Mutation::put(key, &native[*key]))
        .collect();
    let mut stale = comparisons.clone();
    stale.push(Compare::ModRevision(PROBE.to_vec(), 0));
    assert!(
        !source
            .transact(stale, writes.clone())
            .await
            .unwrap()
            .succeeded
    );
    assert_eq!(snapshot(source.as_ref()).await.revision, frozen.revision);
    assert!(
        source
            .transact(comparisons, writes)
            .await
            .unwrap()
            .succeeded
    );
    assert_eq!(durable(snapshot(source.as_ref()).await), native);

    let mut source_pd = start(
        source.clone(),
        &config,
        TcpListener::bind(address).await.unwrap(),
    )
    .await;
    let rolled_back = observe(address).await;
    assert_eq!(rolled_back.0, first.0);
    assert!(rolled_back.1 > moved.1 && rolled_back.2 > moved.2);
    assert_eq!(
        durable(snapshot(source.as_ref()).await)[PROBE],
        b"confirmed on native"
    );
    freeze(&mut source_pd, address).await;
    println!(
        "local cutover cluster {}: IDs {} -> {} -> {} -> {}, TSO {:?} -> {:?} -> {:?} -> {:?}; old lease fenced and acknowledged native write replayed before rollback",
        first.0,
        first.1,
        returned.1,
        moved.1,
        rolled_back.1,
        first.2,
        returned.2,
        moved.2,
        rolled_back.2
    );
}

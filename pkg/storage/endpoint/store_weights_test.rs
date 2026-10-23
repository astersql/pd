// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{NodeState, StoreMeta, StoreState};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, Consistency, MemoryStore, MetaStore, Read};
use std::sync::Arc;

async fn contract(store: Arc<dyn MetaStore>) {
    let endpoint = StorageEndpoint::new(store.clone());
    let base = Base::new(store.as_ref());
    let paths = ClusterPaths::new(25);
    let mut loaded = Vec::new();
    endpoint
        .load_stores(&paths, |s| loaded.push(s))
        .await
        .unwrap();
    assert!(loaded.is_empty());
    // Descending insertion, gaps, three pages and Go's exclusive MAX endpoint.
    for id in (0..205).rev().map(|i| i * 2).chain([u64::MAX]) {
        endpoint
            .save_store_meta(
                &paths,
                &StoreMeta {
                    id,
                    state: match id {
                        2 => StoreState::Offline,
                        4 => StoreState::Tombstone,
                        _ => StoreState::Up,
                    },
                    node_state: NodeState::Serving,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    endpoint
        .save_store_weight(&paths, 2, 2.0, 3.0)
        .await
        .unwrap();
    endpoint
        .save_store_weight(&paths, 4, 0.2, 0.3)
        .await
        .unwrap();
    base.save(paths.store_leader_weight_path(6).as_bytes(), b"")
        .await
        .unwrap();
    endpoint
        .load_stores(&paths, |s| loaded.push(s))
        .await
        .unwrap();
    assert_eq!(loaded.len(), 205);
    for (i, s) in loaded.iter().enumerate() {
        assert_eq!(s.id(), i as u64 * 2);
        assert_eq!(
            s.leader_weight(),
            match i {
                1 => 2.0,
                2 => 0.2,
                _ => 1.0,
            }
        );
        assert_eq!(
            s.region_weight(),
            match i {
                1 => 3.0,
                2 => 0.3,
                _ => 1.0,
            }
        );
    }
    assert_eq!(loaded[1].meta().unwrap().node_state, NodeState::Removing);
    assert_eq!(loaded[2].meta().unwrap().node_state, NodeState::Removed);
    assert_eq!(
        endpoint
            .load_store_meta(&paths, 2)
            .await
            .unwrap()
            .unwrap()
            .node_state,
        NodeState::Serving
    );
    assert_eq!(
        base.load(paths.store_leader_weight_path(4).as_bytes())
            .await
            .unwrap(),
        b"0.2"
    );
    assert_eq!(
        base.load(paths.store_region_weight_path(4).as_bytes())
            .await
            .unwrap(),
        b"0.3"
    );
    let leader = store
        .read(Read::range(
            paths.store_leader_weight_path(4).as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    let region = store
        .read(Read::range(
            paths.store_region_weight_path(4).as_bytes(),
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(
        region.entries[0].mod_revision,
        leader.entries[0].mod_revision + 1
    );
    // Actual Go strconv TestFtoa outputs for format 'f', precision -1.
    for (value, bytes) in [
        (1e23, "100000000000000000000000"),
        (99999999999999974834176.0, "99999999999999970000000"),
        (100000000000000008388608.0, "100000000000000010000000"),
    ] {
        endpoint
            .save_store_weight(&paths, 2, value, -0.0)
            .await
            .unwrap();
        assert_eq!(
            base.load(paths.store_leader_weight_path(2).as_bytes())
                .await
                .unwrap(),
            bytes.as_bytes()
        );
        assert_eq!(
            base.load(paths.store_region_weight_path(2).as_bytes())
                .await
                .unwrap(),
            b"-0"
        );
    }
    for text in ["oops", " 1", "1 ", "1e999", "+NaN", "1__2", "0x1", "1p2"] {
        base.save(
            paths.store_leader_weight_path(2).as_bytes(),
            text.as_bytes(),
        )
        .await
        .unwrap();
        let mut ids = Vec::new();
        assert!(
            endpoint
                .load_stores(&paths, |s| ids.push(s.id()))
                .await
                .is_err(),
            "{text}"
        );
        assert_eq!(ids, [0]); // earlier callbacks survive; failing store is not delivered.
    }
    for (text, value) in [
        ("0x1ep-1", 15.0),
        ("-0x1_ep-1", -15.0),
        ("0x_1p0", 1.0),
        ("1_2.5", 12.5),
        ("1e-999", 0.0),
        ("Infinity", f64::INFINITY),
        ("-inf", f64::NEG_INFINITY),
    ] {
        base.save(
            paths.store_leader_weight_path(2).as_bytes(),
            text.as_bytes(),
        )
        .await
        .unwrap();
        let mut weight = 0.0;
        endpoint
            .load_stores(&paths, |s| {
                if s.id() == 2 {
                    weight = s.leader_weight()
                }
            })
            .await
            .unwrap();
        assert_eq!(weight, value, "{text}");
    }
    endpoint
        .save_store_weight(&paths, 2, f64::NAN, f64::INFINITY)
        .await
        .unwrap();
    assert_eq!(
        base.load(paths.store_leader_weight_path(2).as_bytes())
            .await
            .unwrap(),
        b"NaN"
    );
    assert_eq!(
        base.load(paths.store_region_weight_path(2).as_bytes())
            .await
            .unwrap(),
        b"+Inf"
    );
    endpoint
        .load_stores(&paths, |s| {
            if s.id() == 2 {
                assert!(s.leader_weight().is_nan())
            }
        })
        .await
        .unwrap();
    base.save(paths.store_region_weight_path(2).as_bytes(), b"bad")
        .await
        .unwrap();
    assert!(endpoint.load_stores(&paths, |_| {}).await.is_err());
    base.save(paths.store_path(2).as_bytes(), b"\xff")
        .await
        .unwrap();
    assert!(matches!(
        endpoint.load_stores(&paths, |_| {}).await,
        Err(EndpointError::ProtoUnmarshal(_))
    ));
    // Empty range value decodes to a zero Store (unlike the point-read helper).
    base.save(paths.store_path(2).as_bytes(), b"")
        .await
        .unwrap();
    let mut ids = Vec::new();
    endpoint
        .load_stores(&paths, |s| ids.push(s.id()))
        .await
        .unwrap();
    assert_eq!(&ids[..3], &[0, 0, 4]);
    let mut foreign = 0;
    endpoint
        .load_stores(&ClusterPaths::new(26), |_| foreign += 1)
        .await
        .unwrap();
    assert_eq!(foreign, 0);
}
#[tokio::test]
async fn store_weights_memory() {
    contract(Arc::new(MemoryStore::default())).await;
}
#[tokio::test]
async fn store_weights_etcd() {
    let fixture = endpoint_codec_test::fixture::Fixture::start();
    contract(Arc::new(fixture.store().await)).await;
    partial_write_contract(&fixture).await;
}

#[test]
fn store_weights_float_boundaries() {
    use super::store_weights::{format_weight, parse_weight};
    // Go internal/strconv TestFtoa f/-1 fixtures.
    assert_eq!(format_weight(498484681984085570.0), "498484681984085570");
    for (text, value) in [
        ("0x1p-1074", f64::from_bits(1)),
        ("0x1p-1075", 0.0),
        ("0x3p-1075", f64::from_bits(2)),
        ("0x1.fffffffffffffp1023", f64::MAX),
        ("0x1.00000000000008p0", 1.0),
        ("0x1.00000000000018p0", f64::from_bits(1.0f64.to_bits() + 2)),
        ("-0x0p99999999999999999999", -0.0),
    ] {
        assert_eq!(
            parse_weight(text).unwrap().to_bits(),
            value.to_bits(),
            "{text}"
        );
    }
    for text in [
        "0x1p1024",
        "0x1.fffffffffffff8p1023",
        "0x.p0",
        "0x1p_0",
        "nan_",
        "0_x1p0",
        "0x1p0x",
    ] {
        assert!(parse_weight(text).is_none(), "{text}");
    }
}

// Fail at the TCP boundary before the region request can reach real etcd.
// The first write and all reads still execute against the canonical backend.
async fn partial_write_contract(fixture: &endpoint_codec_test::fixture::Fixture) {
    use astersql_pd_storage_kv::EtcdMetaStore;
    use std::time::Duration;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    use tokio_util::sync::CancellationToken;
    let paths = ClusterPaths::new(27);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let upstream = fixture.endpoint.strip_prefix("http://").unwrap().to_owned();
    let blocked = paths.store_region_weight_path(7).into_bytes();
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let (signal, observed) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let work = async {
            let (downstream, _) = listener.accept().await.unwrap();
            let upstream = TcpStream::connect(upstream).await.unwrap();
            let (mut dr, mut dw) = downstream.into_split();
            let (mut ur, mut uw) = upstream.into_split();
            let requests = async {
                let mut tail = Vec::new();
                let mut buffer = [0; 65536];
                loop {
                    let n = dr.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    tail.extend_from_slice(&buffer[..n]);
                    if tail.windows(blocked.len()).any(|window| window == blocked) {
                        let _ = signal.send(());
                        break;
                    }
                    uw.write_all(&buffer[..n]).await.unwrap();
                    if tail.len() > blocked.len() {
                        tail.drain(..tail.len() - blocked.len());
                    }
                }
            };
            tokio::select! { _ = requests => {}, _ = tokio::io::copy(&mut ur, &mut dw) => {} }
            drop((dr, dw, ur, uw));
            // No reconnect reaches upstream while etcd-client retries.
            std::future::pending::<()>().await;
        };
        tokio::select! { _ = shutdown.cancelled() => {}, _ = work => {} }
    });
    let proxied = EtcdMetaStore::connect(&[address], Duration::from_millis(500))
        .await
        .unwrap();
    let result = StorageEndpoint::new(Arc::new(proxied))
        .save_store_weight(&paths, 7, 2.5, 3.5)
        .await;
    stop.cancel();
    task.await.unwrap();
    assert!(
        observed.await.is_ok(),
        "region request must reach the fault boundary"
    );
    assert!(matches!(result, Err(EndpointError::Store(_))));
    let direct = fixture.store().await;
    let base = Base::new(&direct);
    assert_eq!(
        base.load(paths.store_leader_weight_path(7).as_bytes())
            .await
            .unwrap(),
        b"2.5"
    );
    assert_eq!(
        base.load(paths.store_region_weight_path(7).as_bytes())
            .await
            .unwrap(),
        b""
    );
}

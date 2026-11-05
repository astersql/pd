// Copyright 2026 AsterSQL.
use crate::txn_safe_point_oracle_test::{bytes, records};
use astersql_pd_gc::GCStateManager;
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_keyspace::KeyspaceManager;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage_domain::{BarrierTime, Base, MetaStore, StorageEndpoint};
use serde_json::json;
use std::{sync::Arc, time::Duration};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_047_gc_and_legacy_service_go_differential() {
    let (mut go, cluster) = crate::group_merge_oracle_test::start_bootstrapped_go(None).await;
    let gs: Arc<dyn MetaStore> = Arc::new(
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap(),
    );
    let mut fixture = crate::harness::fixture::Fixture::start();
    let rs: Arc<dyn MetaStore> = Arc::new(fixture.store().await);
    let gp = ClusterPaths::new(cluster);
    let rp = ClusterPaths::new(42);
    let id = NULL_KEYSPACE_ID;
    let m = GCStateManager::new(
        StorageEndpoint::new(rs.clone()),
        rp.clone(),
        Arc::new(KeyspaceManager::new(
            astersql_pd_storage_endpoint::StorageEndpoint::new(rs.clone())
                .keyspace_storage(rp.clone()),
        )),
        tokio::runtime::Handle::current(),
    );
    let role = m.on_node_becomes_leader().await;
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let header = Some(pdpb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let mut report = vec![];
    for (label, txn, old, target, strict, corrupt) in [
        ("strict-advance", 50, 10, 30, true, ""),
        ("strict-lower", 50, 10, 9, true, ""),
        ("strict-upper", 50, 10, 51, true, ""),
        ("strict-equal", 50, 10, 10, true, ""),
        ("compatible-lower", 50, 10, 0, false, ""),
        ("compatible-upper", 50, 10, 51, false, ""),
        ("inconsistent-legacy-lower", 40, 100, 99, false, ""),
        ("maximum", u64::MAX, 10, u64::MAX, false, ""),
        ("gc-syntax", 50, 10, 20, false, "gc"),
        ("txn-range", 50, 10, 20, false, "txn"),
        ("revision-syntax", 50, 10, 20, false, "revision"),
        ("empty-revision-conflict", 50, 10, 20, false, "conflict"),
    ] {
        for (s, p) in [(&gs, &gp), (&rs, &rp)] {
            let ep = StorageEndpoint::new(s.clone());
            let mut tx = ep.begin_gc_state_transaction(p).await.unwrap();
            tx.batch.set_txn_safe_point(id, txn);
            tx.batch.set_gc_safe_point(id, old);
            tx.commit().await.unwrap();
            let bad = match corrupt {
                "gc" => Some((p.gc_safe_point_path(id), b"+1".as_slice())),
                "txn" => Some((
                    p.txn_safe_point_path(id),
                    b"184467440737095516160".as_slice(),
                )),
                "revision" => Some((p.gc_state_revision_path(), b"invalid".as_slice())),
                "conflict" => Some((p.gc_state_revision_path(), b"".as_slice())),
                _ => None,
            };
            if let Some((key, value)) = bad {
                Base::new(s.as_ref())
                    .save(key.as_bytes(), value)
                    .await
                    .unwrap();
            }
        }
        let bg = records(gs.as_ref(), &gp, id).await;
        let br = records(rs.as_ref(), &rp, id).await;
        let (g, r) = if strict {
            let g = client
                .advance_gc_safe_point(pdpb::AdvanceGcSafePointRequest {
                    header: header.clone(),
                    target,
                    keyspace_scope: None,
                })
                .await
                .unwrap()
                .into_inner();
            let r = m.advance_gc_safe_point(id, target).await;
            match (&g.header.as_ref().unwrap().error, &r) {
                (None, Ok(r)) => assert_eq!(
                    (g.old_gc_safe_point, g.new_gc_safe_point),
                    (r.old_gc_safe_point, r.new_gc_safe_point)
                ),
                (Some(g), Err(r)) => assert_eq!(g.message, r.to_string(), "{label}"),
                _ => panic!("{label}: {g:?}/{r:?}"),
            };
            (format!("{g:?}"), format!("{r:?}"))
        } else {
            let g = client
                .update_gc_safe_point(pdpb::UpdateGcSafePointRequest {
                    header: header.clone(),
                    safe_point: target,
                })
                .await
                .map(|x| x.into_inner());
            let r = m.compatible_update_gc_safe_point(id, target).await;
            match (&g, &r) {
                (Ok(g), Ok(r)) => assert_eq!(g.new_safe_point, r.new_gc_safe_point),
                (Err(g), Err(r)) => assert_eq!(g.message(), r.to_string(), "{label}"),
                _ => panic!("{label}: {g:?}/{r:?}"),
            };
            (format!("{g:?}"), format!("{r:?}"))
        };
        let ag = records(gs.as_ref(), &gp, id).await;
        let ar = records(rs.as_ref(), &rp, id).await;
        assert_eq!(bytes(&ag), bytes(&ar), "{label}");
        if r.starts_with("Err") {
            for i in 0..6 {
                assert_eq!(bg[i]["entries"], ag[i]["entries"]);
                assert_eq!(br[i]["entries"], ar[i]["entries"]);
            }
        }
        report.push(json!({"case":label,"txn":txn,"old":old,"target":target,"go":g,"rust":r,"before_go":bg,"before_rust":br,"after_go":ag,"after_rust":ar}));
        if !corrupt.is_empty() {
            for (s, p) in [(&gs, &gp), (&rs, &rp)] {
                Base::new(s.as_ref())
                    .save(p.gc_state_revision_path().as_bytes(), b"100")
                    .await
                    .unwrap();
            }
        }
    }
    // Start the byte-valued legacy service sequence from the same durable points.
    for (s, p) in [(&gs, &gp), (&rs, &rp)] {
        let ep = StorageEndpoint::new(s.clone());
        let mut tx = ep.begin_gc_state_transaction(p).await.unwrap();
        tx.batch.set_txn_safe_point(id, 10);
        tx.batch.set_gc_safe_point(id, 5);
        tx.commit().await.unwrap();
    }
    let cases: Vec<(&str, Vec<u8>, u64, i64)> = vec![
        ("insert", b"backup".to_vec(), 30, i64::MAX),
        ("shrink", b"backup".to_vec(), 20, i64::MAX),
        ("behind-ignored", b"backup".to_vec(), 9, i64::MAX),
        ("raw-id", b"bad\xff\xe2\x82".to_vec(), 25, i64::MAX),
        ("worker-blocked", b"gc_worker".to_vec(), 100, i64::MAX),
        ("worker-invalid-ttl", b"gc_worker".to_vec(), 100, 60),
        ("delete", b"backup".to_vec(), 0, 0),
        ("native-br", b"native_br".to_vec(), 30, i64::MAX),
        ("native-br-delete", b"native_br".to_vec(), 0, -1),
        ("empty-id", vec![], 30, i64::MAX),
        ("finite", b"finite".to_vec(), 40, 60),
        ("saturation-finite", b"long".to_vec(), 40, 9_223_372_036),
        ("saturation-infinite", b"long".to_vec(), 40, 9_223_372_037),
    ];
    for (label, service, target, ttl) in cases {
        let g = client
            .update_service_gc_safe_point(pdpb::UpdateServiceGcSafePointRequest {
                header: header.clone(),
                service_id: service.clone(),
                safe_point: target,
                ttl: ttl,
            })
            .await
            .map(|x| x.into_inner());
        let sec = g
            .as_ref()
            .map(|g| i64::MAX - g.ttl)
            .unwrap_or(1_800_000_000);
        let stored = if service == b"native_br" {
            StorageEndpoint::new(gs.clone())
                .load_global_gc_barrier(&gp, "native_br")
                .await
                .unwrap()
                .map(|b| b.expiration_time)
        } else if service == b"gc_worker" || service.is_empty() {
            None
        } else {
            StorageEndpoint::new(gs.clone())
                .load_gc_barrier_with_service_id(&gp, id, &service)
                .await
                .unwrap()
                .map(|b| b.expiration_time)
        };
        let nanos = if stored
            .flatten()
            .is_some_and(|x| x.unix_seconds() > sec.saturating_add(ttl))
        {
            1_000_000
        } else {
            0
        };
        let r = m
            .compatible_update_service_gc_safe_point(
                id,
                &service,
                target,
                ttl,
                BarrierTime::from_unix(sec, nanos),
            )
            .await;
        match (&g, &r) {
            (Ok(g), Ok(r)) => {
                assert_eq!(g.service_id, r.minimum.service_id, "{label}");
                assert_eq!(g.min_safe_point, r.minimum.safe_point, "{label}");
                assert_eq!(g.ttl, r.minimum.expired_at - sec);
            }
            (Err(g), Err(r)) => assert_eq!(g.message(), r.to_string(), "{label}"),
            _ => panic!("{label}: {g:?}/{r:?}"),
        };
        let ag = records(gs.as_ref(), &gp, id).await;
        let ar = records(rs.as_ref(), &rp, id).await;
        assert_eq!(bytes(&ag), bytes(&ar), "{label}");
        report.push(json!({"case":label,"service":service,"target":target,"ttl":ttl,"go":format!("{g:?}"),"rust":format!("{r:?}"),"clock_seconds":sec,"clock_nanos":nanos,"after_go":ag,"after_rust":ar}));
    }
    role.shutdown().await;
    go.stop().unwrap();
    fixture.command("stop", "stopped");
    fixture.command("start", "restarted");
    go.restart_with_args(&crate::readonly_oracle::pinned_go_binary().unwrap(), &[])
        .unwrap();
    go.ready().unwrap();
    let recovered =
        astersql_pd_storage_kv::EtcdMetaStore::connect(&[go.url.clone()], Duration::from_secs(2))
            .await
            .unwrap();
    let a = records(&recovered, &gp, id).await;
    let b = records(&fixture.store().await, &rp, id).await;
    assert_eq!(bytes(&a), bytes(&b));
    report.push(json!({"recovered_go":a,"recovered_rust":b}));
    if let Some(p) = std::env::var_os("MCS_047_ORACLE_REPORT") {
        std::fs::write(p, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}

// Copyright 2026 AsterSQL.
use crate::{
    harness::Server,
    txn_safe_point_oracle_test::{bytes, records},
};
use astersql_pd_keypath::{ClusterPaths, NULL_KEYSPACE_ID};
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use astersql_pd_storage_domain::{Base, MetaStore, StorageEndpoint};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
type Client = PdClient<tonic::transport::Channel>;
fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
fn check_header(h: Option<pdpb::ResponseHeader>, id: u64) -> Value {
    let mut h = h.unwrap();
    assert_eq!(h.cluster_id, id);
    h.cluster_id = 0;
    json!(format!("{h:?}"))
}
fn status(s: tonic::Status) -> Value {
    json!({"code":format!("{:?}",s.code()),"message":s.message()})
}
async fn get(
    c: &mut Client,
    id: u64,
    ks: u32,
) -> Result<pdpb::GetGcSafePointV2Response, tonic::Status> {
    c.get_gc_safe_point_v2(pdpb::GetGcSafePointV2Request {
        header: header(id),
        keyspace: Some(pdpb::get_gc_safe_point_v2_request::Keyspace::KeyspaceId(ks)),
    })
    .await
    .map(|r| r.into_inner())
}
async fn update(
    c: &mut Client,
    id: u64,
    ks: u32,
    point: u64,
) -> Result<pdpb::UpdateGcSafePointV2Response, tonic::Status> {
    c.update_gc_safe_point_v2(pdpb::UpdateGcSafePointV2Request {
        header: header(id),
        keyspace: Some(pdpb::update_gc_safe_point_v2_request::Keyspace::KeyspaceId(
            ks,
        )),
        safe_point: point,
    })
    .await
    .map(|r| r.into_inner())
}
async fn service(
    c: &mut Client,
    id: u64,
    name: &[u8],
    ks: u32,
    point: u64,
    ttl: i64,
) -> Result<pdpb::UpdateServiceSafePointV2Response, tonic::Status> {
    c.update_service_safe_point_v2(pdpb::UpdateServiceSafePointV2Request {
        header: header(id),
        service_id: name.to_vec(),
        keyspace: Some(pdpb::update_service_safe_point_v2_request::Keyspace::KeyspaceId(ks)),
        safe_point: point,
        ttl,
    })
    .await
    .map(|r| r.into_inner())
}
async fn tso(c: &mut Client, id: u64) -> i64 {
    let mut stream = c
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header: header(id),
            count: 1,
            ..Default::default()
        }]))
        .await
        .unwrap()
        .into_inner();
    stream
        .message()
        .await
        .unwrap()
        .unwrap()
        .timestamp
        .unwrap()
        .physical
        / 1000
}
async fn bootstrap(c: &mut Client, id: u64) {
    let r = c
        .bootstrap(pdpb::BootstrapRequest {
            header: header(id),
            store: Some(metapb::Store {
                id: 1,
                version: "8.5.0".into(),
                ..Default::default()
            }),
            region: Some(metapb::Region {
                id: 2,
                peers: vec![metapb::Peer {
                    id: 3,
                    store_id: 1,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(r.header.unwrap().error.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_004_single_keyspace_behavior() {
    assert_eq!(std::env::var("PD_COMPAT_MODE").as_deref(), Ok("dual"));
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = std::path::PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut reports = vec![];
    let mut samples = vec![];
    for is_rust in [false, true] {
        let mut server = if is_rust {
            Server::start_rust(&rust).unwrap()
        } else {
            Server::start(&go).unwrap()
        };
        server.ready().unwrap();
        let mut c = PdClient::connect(server.url.clone()).await.unwrap();
        let id = c
            .get_members(pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        let store: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[server.metadata_url().to_string()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        let p = ClusterPaths::new(id);
        let ep = StorageEndpoint::new(store.clone());
        let mut values = vec![];
        let mut detail = vec![];
        for h in [
            get(&mut c, id, 2)
                .await
                .expect("GCv2 read must reach its manager")
                .header,
            update(&mut c, id, 2, 10).await.unwrap().header,
            service(&mut c, id, b"backup", 2, 10, 60)
                .await
                .unwrap()
                .header,
        ] {
            assert_eq!(
                h.as_ref().unwrap().error.as_ref().unwrap().r#type,
                pdpb::ErrorType::NotBootstrapped as i32
            );
            values.push(check_header(h, id));
        }
        bootstrap(&mut c, id).await;
        let keys = astersql_pd_storage_endpoint::StorageEndpoint::new(store.clone())
            .keyspace_storage(p.clone());
        for (ks, level) in [(2, true), (3, true), (4, false)] {
            let mut tx = Base::new(store.as_ref()).begin_txn();
            keys.save_keyspace_meta(
                &mut tx,
                &p,
                &astersql_pd_storage_domain::Keyspace {
                    identity: Some(astersql_pd_storage_domain::KeyspaceIdentity::Id(ks)),
                    name: format!("space{ks}"),
                    config: if level {
                        std::collections::HashMap::from([(
                            "gc_management_type".into(),
                            "keyspace_level".into(),
                        )])
                    } else {
                        Default::default()
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            tx.commit::<String>().await.unwrap();
        }
        let mut tx = ep.begin_gc_state_transaction(&p).await.unwrap();
        for ks in [2, 3, NULL_KEYSPACE_ID] {
            tx.batch.set_txn_safe_point(ks, 50);
            tx.batch.set_gc_safe_point(ks, 0);
        }
        tx.commit().await.unwrap();
        for (ks, target) in [
            (2, 20),
            (3, 30),
            (2, 20),
            (2, 10),
            (2, 51),
            (4, 20),
            (99, 20),
            (0xfffffffe, 15),
        ] {
            let before = records(store.as_ref(), &p, ks).await;
            let result = match update(&mut c, id, ks, target).await {
                Ok(r) => json!({"header":check_header(r.header,id),"new":r.new_safe_point}),
                Err(e) => status(e),
            };
            let after = records(store.as_ref(), &p, ks).await;
            if [10, 51].contains(&target) || [4, 99].contains(&ks) {
                assert_eq!(
                    before
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| &v["entries"])
                        .collect::<Vec<_>>(),
                    after
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| &v["entries"])
                        .collect::<Vec<_>>()
                );
            }
            values.push(json!({"ks":ks,"target":target,"response":result,"state":bytes(&after)}));
            detail.push(json!({"before":before,"after":after}));
        }
        for ks in [2, 3, 4, 99, 0xfffffffe] {
            let result = match get(&mut c, id, ks).await {
                Ok(r) => json!({"header":check_header(r.header,id),"point":r.safe_point}),
                Err(e) => status(e),
            };
            values.push(result);
        }
        assert_eq!(get(&mut c, id, 2).await.unwrap().safe_point, 20);
        assert_eq!(get(&mut c, id, 3).await.unwrap().safe_point, 30);
        for (ks, name, point, ttl) in [
            (2, b"backup".as_slice(), 55, i64::MAX),
            (3, b"backup".as_slice(), 65, i64::MAX),
            (2, b"gc_worker".as_slice(), 25, 60),
            (4, b"backup".as_slice(), 25, 60),
            (99, b"backup".as_slice(), 25, 60),
            (2, b"backup".as_slice(), 0, 0),
        ] {
            let before = records(store.as_ref(), &p, ks).await;
            let t0 = tso(&mut c, id).await;
            let result = match service(&mut c, id, name, ks, point, ttl).await {
                Ok(r) => {
                    let t1 = tso(&mut c, id).await;
                    assert!((i64::MAX - t1..=i64::MAX - t0).contains(&r.ttl));
                    detail.push(json!({"raw_service_response":format!("{r:?}"),"tso_before":t0,"tso_after":t1}));
                    json!({"header":check_header(r.header,id),"service":r.service_id,"point":r.min_safe_point})
                }
                Err(e) => status(e),
            };
            let after = records(store.as_ref(), &p, ks).await;
            values.push(json!({"ks":ks,"name":name,"response":result,"state":bytes(&after)}));
            detail.push(json!({"before":before,"after":after}));
        }
        let durable = [
            records(store.as_ref(), &p, 2).await,
            records(store.as_ref(), &p, 3).await,
        ];
        drop(c);
        drop(ep);
        drop(keys);
        drop(store);
        if is_rust {
            server.restart_rust(&rust).unwrap()
        } else {
            server.restart_with_args(&go, &[]).unwrap()
        };
        server.ready().unwrap();
        c = PdClient::connect(server.url.clone()).await.unwrap();
        assert_eq!(get(&mut c, id, 2).await.unwrap().safe_point, 20);
        assert_eq!(get(&mut c, id, 3).await.unwrap().safe_point, 30);
        let recovered: Arc<dyn MetaStore> = Arc::new(
            astersql_pd_storage_kv::EtcdMetaStore::connect(
                &[server.metadata_url().to_string()],
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
        );
        for (index, ks) in [2, 3].into_iter().enumerate() {
            let after = records(recovered.as_ref(), &p, ks).await;
            for n in 0..6 {
                assert_eq!(durable[index][n]["entries"], after[n]["entries"]);
            }
            detail.push(json!({"wal_recovered":ks,"before":durable[index],"after":after}));
        }
        drop(c);
        reports.push(json!({"rust":is_rust,"raw":detail,"observations":values}));
        samples.push(values);
    }
    if let Some(path) = std::env::var_os("PD_SA004_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    }
    assert_eq!(samples[0], samples[1]);
}

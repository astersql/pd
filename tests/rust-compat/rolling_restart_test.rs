// Copyright 2026 AsterSQL.
use crate::rolling_restart::*;
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_098_behavior() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rust = std::env::var_os("PD_COMPAT_RUST_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap()).join("debug/pd-server")
        });
    scenario(&rust, false).await;
    if std::env::var("PD_COMPAT_MODE").as_deref() == Ok("dual") {
        let go = std::env::var_os("PD_COMPAT_GO_BIN")
            .map(PathBuf::from)
            .unwrap_or(root.join("bin/pd-server"));
        scenario(&go, true).await;
    }
}
struct DataDirectory(PathBuf);
impl Drop for DataDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn scenario(binary: &std::path::Path, go: bool) {
    let fixture = crate::harness::fixture::Fixture::start();
    let reservations: Vec<_> = (0..6)
        .map(|_| std::net::TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let urls: Vec<_> = reservations
        .iter()
        .map(|s| format!("http://{}", s.local_addr().unwrap()))
        .collect();
    let topology = (0..3)
        .map(|i| format!("rolling98-{i}={}", urls[3 + i]))
        .collect::<Vec<_>>()
        .join(",");
    let data = PathBuf::from(
        std::env::var_os("PD_COMPAT_WORK_DIR")
            .unwrap_or_else(|| std::env::var_os("CARGO_TARGET_DIR").unwrap()),
    )
    .join(format!("rolling98-{}-{}", std::process::id(), go));
    std::fs::create_dir_all(&data).unwrap();
    let _data_guard = DataDirectory(data.clone());
    // Compare shared durable metadata; Go's optional local region cache is a separate topology.
    let config = data.join("go.toml");
    std::fs::write(&config, "[pd-server]\nuse-region-storage = false\n").unwrap();
    let mut args = Vec::new();
    for i in 0..3 {
        let mut a = vec![
            format!("--name=rolling98-{i}"),
            format!("--client-urls={}", urls[i]),
            format!("--advertise-client-urls={}", urls[i]),
            format!("--peer-urls={}", urls[3 + i]),
            format!("--advertise-peer-urls={}", urls[3 + i]),
        ];
        if go {
            a.extend([
                format!("--config={}", config.display()),
                format!("--initial-cluster={topology}"),
                format!("--data-dir={}", data.join(i.to_string()).display()),
            ]);
        } else {
            a.push(format!("--etcd-endpoints={}", fixture.endpoint));
        }
        args.push(a);
    }
    drop(reservations);
    let mut servers: Vec<_> = args.iter().map(|a| launch(binary, a)).collect();
    let mut clients = Vec::new();
    for url in &urls[..3] {
        let endpoint = tonic::transport::Endpoint::from_shared(url.clone())
            .unwrap()
            .timeout(Duration::from_secs(2));
        clients.push(PdClient::new(endpoint.connect_lazy()));
    }
    let mut alive = vec![true; 3];
    let (leader, initial) = wait_leader(&mut clients, &alive).await;
    assert_eq!(initial.members.len(), 3);
    let mut identities: Vec<_> = initial
        .members
        .iter()
        .map(|m| {
            (
                m.name.clone(),
                m.member_id,
                m.client_urls.clone(),
                m.peer_urls.clone(),
            )
        })
        .collect();
    identities.sort();
    let cluster_id = initial.header.unwrap().cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id,
        ..Default::default()
    });
    let peer = metapb::Peer {
        id: 3,
        store_id: 1,
        ..Default::default()
    };
    let region = metapb::Region {
        id: 2,
        peers: vec![peer.clone()],
        region_epoch: Some(metapb::RegionEpoch {
            conf_ver: 1,
            version: 1,
        }),
        ..Default::default()
    };
    let result = clients[leader]
        .bootstrap(pdpb::BootstrapRequest {
            header: header.clone(),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:29998".into(),
                ..Default::default()
            }),
            region: Some(region.clone()),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(result.header.unwrap().error.is_none());
    let mut ids = BTreeSet::new();
    let mut previous = (0, 0);
    let mut last_id = 0;
    // Stop each member once, beginning with the leader; clients/channels survive every restart.
    let order = [leader, (leader + 1) % 3, (leader + 2) % 3];
    for stop in order {
        servers[stop]
            .stop_with_timeout(Duration::from_secs(20))
            .unwrap();
        alive[stop] = false;
        // Sample both while a member is absent and after its exact identity restarts.
        for sample in 0..4 {
            if sample == 3 {
                servers[stop].resume_with_args(binary, &args[stop]).unwrap();
                alive[stop] = true;
                tokio::time::timeout(Duration::from_secs(40), async {
                    loop {
                        if let Ok(r) = clients[stop]
                            .get_members(pdpb::GetMembersRequest::default())
                            .await
                        {
                            if r.into_inner()
                                .header
                                .is_some_and(|h| h.cluster_id == cluster_id)
                            {
                                break;
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                })
                .await
                .expect("restarted member did not reconnect on original channel");
            }
            let (current, _) = wait_leader(&mut clients, &alive).await;
            let id = clients[current]
                .alloc_id(pdpb::AllocIdRequest {
                    header: header.clone(),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            assert!(id.header.unwrap().error.is_none());
            assert!(id.id > last_id);
            assert!(ids.insert(id.id));
            last_id = id.id;
            let mut stream = clients[current]
                .tso(tokio_stream::iter([pdpb::TsoRequest {
                    header: header.clone(),
                    count: 1,
                    ..Default::default()
                }]))
                .await
                .unwrap()
                .into_inner();
            let r = stream.message().await.unwrap().unwrap();
            assert!(r.header.unwrap().error.is_none());
            let ts = r.timestamp.unwrap();
            assert!((ts.physical, ts.logical) > previous);
            previous = (ts.physical, ts.logical);
            let hb = clients[current]
                .store_heartbeat(pdpb::StoreHeartbeatRequest {
                    header: header.clone(),
                    stats: Some(pdpb::StoreStats {
                        store_id: 1,
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .await
                .unwrap()
                .into_inner();
            assert!(hb.header.unwrap().error.is_none());
            tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let found = clients[current]
                        .get_region(pdpb::GetRegionRequest {
                            header: header.clone(),
                            region_key: vec![],
                            ..Default::default()
                        })
                        .await
                        .unwrap()
                        .into_inner();
                    assert!(found.header.unwrap().error.is_none());
                    if let Some(restored) = found.region {
                        assert_eq!(restored, region);
                        break;
                    }
                    // Go publishes leadership before the region cache finishes loading.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .expect("persisted region missing after leader change");
        }
        let (_, m) = wait_leader(&mut clients, &alive).await;
        assert_eq!(m.header.unwrap().cluster_id, cluster_id);
        let mut after: Vec<_> = m
            .members
            .iter()
            .map(|m| {
                (
                    m.name.clone(),
                    m.member_id,
                    m.client_urls.clone(),
                    m.peer_urls.clone(),
                )
            })
            .collect();
        after.sort();
        assert_eq!(after, identities);
        let output = std::process::Command::new("curl")
            .args([
                "-fsS",
                "--max-time",
                "3",
                &format!("{}/pd/api/v1/members", urls[stop]),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        let http: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(http["members"].as_array().unwrap().len(), 3);
        // A bad cluster ID is rejected without poisoning the surviving channel.
        let (current, _) = wait_leader(&mut clients, &alive).await;
        assert!(
            clients[current]
                .alloc_id(pdpb::AllocIdRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id: cluster_id ^ 1,
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .await
                .is_err()
        );
        // Cancel a real idle TSO stream while preserving its parent channel.
        let (tx, rx) = tokio::sync::mpsc::channel::<pdpb::TsoRequest>(1);
        tx.send(pdpb::TsoRequest {
            header: header.clone(),
            count: 1,
            ..Default::default()
        })
        .await
        .unwrap();
        let mut stream = clients[current]
            .tso(tokio_stream::wrappers::ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner();
        let response = stream.message().await.unwrap().unwrap();
        let ts = response.timestamp.unwrap();
        assert!((ts.physical, ts.logical) > previous);
        previous = (ts.physical, ts.logical);
        drop((tx, stream));
        assert!(
            clients[current]
                .get_members(pdpb::GetMembersRequest::default())
                .await
                .is_ok()
        );
    }
    for s in &mut servers {
        s.stop_with_timeout(Duration::from_secs(20)).unwrap();
    }
    drop(servers);
    std::fs::remove_dir_all(data).unwrap();
    println!(
        "task098 {}: three rolling restarts, {} unique IDs, monotone TSO, heartbeat and durable region/identity",
        if go { "Go" } else { "Rust" },
        ids.len()
    );
}

// Copyright 2026 AsterSQL.
use crate::{go_client_acceptance::*, rolling_restart::*};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::{path::PathBuf, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_099_behavior() {
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").expect("claim a Cargo slot"));
    for name in ["pd-region-bench", "pd-tso-bench", "pd-heartbeat-bench"] {
        assert!(
            target.join(name).is_file(),
            "existing Go tool required: {name}"
        );
    }
    let rust = std::env::var_os("PD_COMPAT_RUST_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| target.join("debug/pd-server"));
    if std::env::var("PD_COMPAT_MODE").as_deref() == Ok("dual") {
        scenario(
            PathBuf::from(std::env::var_os("PD_COMPAT_GO_BIN").unwrap()),
            true,
            &target,
        )
        .await;
    }
    scenario(rust, false, &target).await;
}

async fn scenario(binary: PathBuf, go: bool, target: &std::path::Path) {
    let fixture = crate::harness::fixture::Fixture::start();
    let reservations: Vec<_> = (0..6)
        .map(|_| std::net::TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let urls: Vec<_> = reservations
        .iter()
        .map(|s| format!("http://{}", s.local_addr().unwrap()))
        .collect();
    // Reuse task098's stable member names and leader-readiness observer.
    let topology = (0..3)
        .map(|i| format!("rolling98-{i}={}", urls[3 + i]))
        .collect::<Vec<_>>()
        .join(",");
    let data = Directory(target.join(format!("go-client99-{}-{go}", std::process::id())));
    std::fs::create_dir_all(&data.0).unwrap();
    let config = data.0.join("go.toml");
    std::fs::write(&config, "[pd-server]\nuse-region-storage = false\n").unwrap();
    let args: Vec<Vec<String>> = (0..3)
        .map(|i| {
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
                    format!("--data-dir={}", data.0.join(i.to_string()).display()),
                ]);
            } else {
                a.push(format!("--etcd-endpoints={}", fixture.endpoint));
            }
            a
        })
        .collect();
    drop(reservations);
    let mut servers: Vec<_> = args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let mut server = launch(&binary, a);
            server.url = urls[i].clone();
            server
        })
        .collect();
    let mut clients: Vec<_> = urls[..3]
        .iter()
        .map(|url| {
            PdClient::new(
                tonic::transport::Endpoint::from_shared(url.clone())
                    .unwrap()
                    .timeout(Duration::from_secs(2))
                    .connect_lazy(),
            )
        })
        .collect();
    let mut alive = vec![true; 3];
    let (leader, members) = wait_leader(&mut clients, &alive).await;
    let cluster_id = members.header.unwrap().cluster_id;
    let heartbeat_config = data.0.join("heartbeat.toml");
    std::fs::write(
        &heartbeat_config,
        "store-count = 3\nregion-count = 1\nreplica = 3\nleader-update-ratio = 0.0\n",
    )
    .unwrap();
    let mut heartbeat = GoTool::start(
        &target.join("pd-heartbeat-bench"),
        &[
            format!("--pd-endpoints={}", urls[leader]),
            format!("--config={}", heartbeat_config.display()),
            "--status-addr=127.0.0.1:0".into(),
            "--metrics-addr=127.0.0.1:0".into(),
        ],
        data.0.join("heartbeat.log"),
    );
    // The unmodified tool reports every 60 seconds. Observe the actual committed report.
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let reply = clients[leader]
                .scan_regions(pdpb::ScanRegionsRequest {
                    header: Some(pdpb::RequestHeader {
                        cluster_id,
                        ..Default::default()
                    }),
                    limit: 100,
                    ..Default::default()
                })
                .await;
            if reply.is_ok_and(|r| {
                r.into_inner()
                    .regions
                    .iter()
                    .any(|r| r.region.as_ref().is_some_and(|r| r.peers.len() == 3))
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("Go heartbeat did not commit: {}", heartbeat.output()));
    heartbeat.cancel().await;
    let mut region = GoTool::start(
        &target.join("pd-region-bench"),
        &[
            format!("-pd={}", urls[leader]),
            "-region-count=1".into(),
            "-c=1".into(),
            "-duration=120s".into(),
            "-interval=1s".into(),
        ],
        data.0.join("region.log"),
    );
    region.wait_samples(0).await;
    let mut tso = GoTool::start(
        &target.join("pd-tso-bench"),
        &[
            format!("-pd={}", urls[leader]),
            "-c=1".into(),
            "-duration=120s".into(),
            "-interval=1s".into(),
            "-max-send-interval-ms=10".into(),
            "-v".into(),
        ],
        data.0.join("tso.log"),
    );
    tso.wait_samples(0).await;
    observe(&mut clients[leader], cluster_id).await;
    servers[leader]
        .stop_with_timeout(Duration::from_secs(20))
        .unwrap();
    alive[leader] = false;
    let (next, _) = wait_leader(&mut clients, &alive).await;
    assert_ne!(next, leader);
    // Wait beyond any in-flight pre-election sample; the same client processes must recover.
    let region_samples = region.samples();
    let tso_samples = tso.samples();
    region.wait_samples(region_samples + 1).await;
    tso.wait_samples(tso_samples + 1).await;
    observe(&mut clients[next], cluster_id).await;
    let (http_status, http_members) = servers[next].get("/pd/api/v1/members").unwrap();
    assert_eq!(http_status, 200);
    assert_eq!(http_members["members"].as_array().unwrap().len(), 3);
    let region_offset = region.output().len();
    let previous = region.samples();
    region.wait_samples(previous + 1).await;
    let stable = region.output();
    assert!(
        !stable[region_offset..].contains("get region error"),
        "Go region reads failed after election: {}",
        &stable[region_offset..]
    );
    assert!(
        stable.contains(&format!("new-leader={}", urls[next])),
        "Go region client did not discover new leader"
    );
    assert!(
        tso.output().contains(&format!("new-url={}", urls[next])),
        "Go TSO client did not discover new leader"
    );
    println!(
        "Go clients against go={go}: cluster={cluster_id}, leader={leader}->{next}, region_samples={}, tso_samples={}",
        region.samples(),
        tso.samples()
    );
    region.cancel().await;
    tso.cancel().await;
    for (i, server) in servers.iter_mut().enumerate() {
        if alive[i] {
            server.stop_with_timeout(Duration::from_secs(20)).unwrap();
        }
    }
}

async fn observe(client: &mut PdClient<tonic::transport::Channel>, cluster_id: u64) {
    let header = Some(pdpb::RequestHeader {
        cluster_id,
        ..Default::default()
    });
    let stores = client
        .get_all_stores(pdpb::GetAllStoresRequest {
            header: header.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(stores.header.unwrap().error.is_none());
    assert_eq!(
        stores.stores.len(),
        3,
        "Go PutStores must persist all three stores"
    );
    let bootstrap = client
        .is_bootstrapped(pdpb::IsBootstrappedRequest {
            header: header.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(
        bootstrap.bootstrapped,
        "bootstrap must be performed by the Go tool"
    );
    let scan = client
        .scan_regions(pdpb::ScanRegionsRequest {
            header,
            limit: 100,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(scan.header.unwrap().error.is_none());
    assert_eq!(
        scan.regions.len(),
        1,
        "Go heartbeat regions must survive leader change"
    );
    let region = scan.regions[0].region.as_ref().unwrap();
    assert_eq!(region.id, 1);
    assert!(region.start_key.is_empty() && region.end_key.is_empty());
    let epoch = region.region_epoch.as_ref().unwrap();
    assert_eq!((epoch.conf_ver, epoch.version), (2, 1));
    assert_eq!(
        region
            .peers
            .iter()
            .map(|p| (p.id, p.store_id))
            .collect::<Vec<_>>(),
        vec![(2, 1), (3, 2), (4, 3)]
    );
}

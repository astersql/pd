// Copyright 2026 AsterSQL.

use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
use serde_json::{Value, json};
use std::{process::Command, sync::Arc};

pub(crate) fn http(url: &str, method: &str, body: Value) -> (u16, Value) {
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            "40",
            "-X",
            method,
            "-H",
            "Content-Type: application/json",
            "-d",
            &body.to_string(),
            "-w",
            "\n%{http_code}",
            url,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let (body, status) = text.rsplit_once('\n').unwrap();
    (
        status.parse().unwrap(),
        serde_json::from_str(body).unwrap_or_else(|_| json!(body)),
    )
}

/// Explicit environment test: no silent success when the admitted oracle is absent.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires admitted read-only Go binary and official etcd"]
async fn mcs_027_readonly_go_reservations() {
    let binary = crate::readonly_oracle::pinned_go_binary().unwrap();
    let mut go = crate::harness::Server::start(&binary).unwrap();
    go.ready().unwrap();
    let mut client = PdClient::connect(go.url.clone()).await.unwrap();
    let id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let response = client
        .bootstrap(pdpb::BootstrapRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: id,
                ..Default::default()
            }),
            store: Some(metapb::Store {
                id: 1,
                address: "127.0.0.1:20160".into(),
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
    assert!(response.header.unwrap().error.is_none());
    assert_eq!(
        http(
            &format!("{}/pd/api/v1/config", go.url),
            "POST",
            json!({"keyspace.wait-region-split": false})
        )
        .0,
        200
    );
    let fixture = crate::harness::fixture::Fixture::start();
    let groups = format!("{}/pd/api/v2/meta-service-groups", go.url);
    assert_eq!(
        http(&groups, "PATCH", json!({"a": fixture.endpoint})).0,
        200
    );
    assert_eq!(
        http(
            &format!("{groups}/a/status"),
            "PATCH",
            json!({"enabled": true})
        )
        .0,
        200
    );
    let rust_store: Arc<dyn astersql_pd_storage_kv::MetaStore> = Arc::new(fixture.store().await);
    let paths = astersql_pd_keypath::ClusterPaths::new(42);
    let endpoint = astersql_pd_storage_endpoint::StorageEndpoint::new(rust_store.clone());
    astersql_pd_storage_kv::Base::new(rust_store.as_ref())
        .run_in_txn(async |txn| {
            endpoint.save_meta_service_group_status(
                txn,
                &paths,
                "a",
                Some(&astersql_pd_storage_endpoint::MetaServiceGroupStatus {
                    assignment_count: 0,
                    enabled: true,
                }),
            )
        })
        .await
        .unwrap();
    let manager = astersql_pd_keyspace::MetaServiceGroupManager::new(
        rust_store.clone(),
        paths.clone(),
        [("a".into(), fixture.endpoint.clone())].into(),
    );
    let mut transcript = vec![];
    for n in 0..3 {
        let created = http(
            &format!("{}/pd/api/v2/keyspaces", go.url),
            "POST",
            json!({"name":format!("sa003-dependency-{n}")}),
        );
        assert_eq!(created.0, 200, "{created:?}");
        assert_eq!(manager.pick_group().await.unwrap(), "a");
        let (_, status) = go.get("/pd/api/v2/meta-service-groups").unwrap();
        let go_count = status
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == "a")
            .unwrap()["assigned_keyspaces"]
            .as_i64()
            .unwrap();
        let bytes = astersql_pd_storage_kv::Base::new(rust_store.as_ref())
            .load(paths.meta_service_group_status_path("a").as_bytes())
            .await
            .unwrap();
        let rust_status: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(go_count, rust_status["assignment_count"].as_i64().unwrap());
        transcript.push(json!({"create":created,"go_status":status,"rust_status":rust_status}));
    }
    let duplicate = http(
        &format!("{}/pd/api/v2/keyspaces", go.url),
        "POST",
        json!({"name":"sa003-dependency-0"}),
    );
    assert_eq!(duplicate.0, 500);
    let (_, status) = go.get("/pd/api/v2/meta-service-groups").unwrap();
    assert_eq!(status[0]["assigned_keyspaces"], 3);
    transcript.push(json!({"duplicate":duplicate,"after":status}));
    go.stop().unwrap();
    if let Some(path) = std::env::var_os("MCS_027_ORACLE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&transcript).unwrap()).unwrap();
    }
}

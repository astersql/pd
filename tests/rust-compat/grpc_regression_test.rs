// Copyright 2026 AsterSQL.
use super::grpc_regression::cases;
use std::collections::BTreeSet;

#[test]
fn task_100_behavior() {
    let inventory = include_str!("../../docs/rust/server-api/wire-inventory.md");
    let expected: BTreeSet<_> = inventory
        .lines()
        .filter(|l| l.starts_with("| ") && l.contains("RPC-"))
        .filter_map(|line| {
            let columns: Vec<_> = line.split('|').collect();
            let method = columns[1].trim();
            (columns[5].trim() != "78" || matches!(method, "WatchGCSafePointV2" | "WatchGCStates"))
                .then_some(method)
        })
        .collect();
    let actual: BTreeSet<_> = cases()
        .iter()
        .flat_map(|(_, methods)| methods.iter().copied())
        .collect();
    assert_eq!(
        actual, expected,
        "every implemented PD matrix method needs a real differential entrypoint"
    );
    super::grpc_regression::run(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
}

/// Mutate actual generated messages, including optional defaults and enum numbers;
/// comparisons must retain every field after the explicit identity normalization.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_100_negative_controls() {
    use crate::{
        harness::Server,
        normalize::{Rules, differences, tso},
    };
    use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
    use prost::Message;
    use serde_json::json;
    let target = std::path::PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap());
    let mut server = Server::start_rust(&target.join("debug/pd-server")).unwrap();
    server.ready().unwrap();
    let mut client = PdClient::connect(server.url.clone()).await.unwrap();
    let response = client
        .get_cluster_info(pdpb::GetClusterInfoRequest::default())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        response.service_modes,
        vec![pdpb::ServiceMode::PdSvcMode as i32]
    );
    let mut changed = response.clone();
    changed.service_modes[0] = i32::MAX;
    assert_ne!(response, changed, "unknown enum value must survive");
    changed = response.clone();
    changed.tso_urls.push("unexpected".into());
    assert_ne!(response, changed);
    changed = response.clone();
    changed.header.as_mut().unwrap().error = Some(pdpb::Error::default());
    assert_ne!(
        response, changed,
        "present default error differs from absence"
    );
    changed = response.clone();
    changed.header = None;
    assert_ne!(response, changed, "default message presence must survive");
    let status = client
        .get_store(pdpb::GetStoreRequest::default())
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    let baseline =
        json!({"code":status.code() as i32,"message":status.message(),"header":{"cluster_id":123}});
    let rules = Rules(vec![(json!(123), json!(0))]);
    for key in ["code", "message"] {
        let mut changed = baseline.clone();
        changed[key] = if key == "code" {
            json!(tonic::Code::Unavailable as i32)
        } else {
            json!("changed transport cause")
        };
        assert!(!differences(&rules.apply(&baseline), &rules.apply(&changed)).is_empty());
    }
    let a = pdpb::TsoResponse {
        header: Some(pdpb::ResponseHeader {
            cluster_id: 123,
            error: None,
        }),
        count: 1,
        timestamp: Some(pdpb::Timestamp {
            physical: 500,
            logical: 7,
            suffix_bits: 0,
        }),
    };
    let baseline = tso(&a.encode_to_vec(), 500).unwrap();
    for field in 0..6 {
        let mut b = a.clone();
        match field {
            0 => {
                b.header.as_mut().unwrap().error = Some(pdpb::Error {
                    r#type: i32::MAX,
                    message: "error".into(),
                })
            }
            1 => b.count = 0,
            2 => b.timestamp.as_mut().unwrap().logical += 1,
            3 => b.timestamp.as_mut().unwrap().suffix_bits = 1,
            4 => b.timestamp = None,
            _ => b.timestamp.as_mut().unwrap().physical += 1,
        }
        assert!(
            !differences(&baseline, &tso(&b.encode_to_vec(), 500).unwrap()).is_empty(),
            "TSO mutation {field} swallowed"
        );
    }
    let mut with_error = a.clone();
    with_error.header.as_mut().unwrap().error = Some(pdpb::Error {
        r#type: pdpb::ErrorType::Unknown as i32,
        message: "original cause".into(),
    });
    let original = tso(&with_error.encode_to_vec(), 500).unwrap();
    let mut changed = with_error.clone();
    changed
        .header
        .as_mut()
        .unwrap()
        .error
        .as_mut()
        .unwrap()
        .r#type = pdpb::ErrorType::NotBootstrapped as i32;
    assert!(!differences(&original, &tso(&changed.encode_to_vec(), 500).unwrap()).is_empty());
    changed = with_error;
    changed
        .header
        .as_mut()
        .unwrap()
        .error
        .as_mut()
        .unwrap()
        .message = "changed cause".into();
    assert!(!differences(&original, &tso(&changed.encode_to_vec(), 500).unwrap()).is_empty());
    // Identity rules cannot rewrite errors or unrelated state values even if their
    // raw scalar happens to equal the registered identity.
    let value = json!({"cluster_id":123,"error":{"type":123,"message":"123"},"state":123});
    assert_eq!(
        rules.apply(&value),
        json!({"cluster_id":0,"error":{"type":123,"message":"123"},"state":123})
    );
    server.stop().unwrap();
}

#[test]
fn task_100_sync_terminal_recording() {
    use crate::harness::verify_sync_terminal_frame;
    use astersql_pd_proto::pdpb;
    let saved: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/task-100-sync-close.json")).unwrap();
    let data = &saved["frame"];
    let expected_keys: BTreeSet<_> = [
        "header",
        "start_index",
        "regions",
        "region_stats",
        "region_leaders",
        "buckets",
        "down_peers",
        "pending_peers",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        data.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        expected_keys
    );
    for field in [
        "regions",
        "region_stats",
        "region_leaders",
        "buckets",
        "down_peers",
        "pending_peers",
    ] {
        assert!(data[field].as_array().unwrap().is_empty());
    }
    let id = data["header"]["cluster_id"].as_u64().unwrap();
    let frame = pdpb::SyncRegionResponse {
        header: Some(pdpb::ResponseHeader {
            cluster_id: id,
            error: Some(pdpb::Error {
                r#type: data["header"]["error"]["type"]
                    .as_i64()
                    .unwrap()
                    .try_into()
                    .unwrap(),
                message: data["header"]["error"]["message"].as_str().unwrap().into(),
            }),
        }),
        start_index: data["start_index"].as_u64().unwrap(),
        ..Default::default()
    };
    verify_sync_terminal_frame(&frame, id).unwrap();
    for change in 0..12 {
        let mut altered = frame.clone();
        match change {
            0 => altered.header = None,
            1 => altered.header.as_mut().unwrap().cluster_id += 1,
            2 => altered.header.as_mut().unwrap().error = None,
            3 => {
                altered
                    .header
                    .as_mut()
                    .unwrap()
                    .error
                    .as_mut()
                    .unwrap()
                    .r#type = pdpb::ErrorType::Ok as i32
            }
            4 => altered
                .header
                .as_mut()
                .unwrap()
                .error
                .as_mut()
                .unwrap()
                .message
                .push('!'),
            5 => altered.start_index = 1,
            6 => altered.regions.push(Default::default()),
            7 => altered.region_stats.push(Default::default()),
            8 => altered.region_leaders.push(Default::default()),
            9 => altered.buckets.push(Default::default()),
            10 => altered.down_peers.push(Default::default()),
            _ => altered.pending_peers.push(Default::default()),
        }
        assert!(
            verify_sync_terminal_frame(&altered, id).is_err(),
            "terminal field mutation {change} swallowed"
        );
    }
}

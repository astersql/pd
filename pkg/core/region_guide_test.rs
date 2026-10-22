// Copyright 2026 AsterSQL.
use crate::*;
#[test]
fn core_task_35_new_region() {
    let r = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    assert_eq!(
        region_guide(&r, None),
        RegionGuide {
            save_kv: true,
            save_cache: true,
            need_sync: false,
            retained: true
        }
    );
}

#[test]
fn core_task_35_branch_matrix() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_35.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let peer = Peer {
            id: 1,
            store_id: 1,
            ..Default::default()
        };
        let mut origin = RegionInfo::new(
            RegionMeta {
                peers: vec![peer.clone()],
                epoch: Some(RegionEpoch {
                    version: 1,
                    conf_ver: 1,
                }),
                ..Default::default()
            },
            Some(peer.clone()),
            vec![],
            vec![],
        );
        origin.stats.flow_round_divisor = 100;
        if matches!(name, "bucket_disable" | "bucket_decrease") {
            origin.update_buckets(
                Some(&RegionBuckets {
                    version: 2,
                    ..Default::default()
                }),
                None,
            );
        }
        if matches!(name, "down_seconds") {
            origin.down_peers.push(DownPeer {
                peer: peer.clone(),
                down_seconds: 1,
            });
        }
        if name == "pending_role" {
            origin.pending_peers.push(peer.clone());
        }
        if matches!(name, "replication_id" | "unknown_replication") {
            origin.set_replication_status(1, 1);
        }
        let mut r = origin.clone();
        match name {
            "unchanged" => {}
            "version" => r.meta.epoch.as_mut().unwrap().version += 1,
            "confver" => r.meta.epoch.as_mut().unwrap().conf_ver += 1,
            "stale_epoch" => r.meta.epoch = None,
            "leader" => r.leader = None,
            "leader_store_only" => r.leader.as_mut().unwrap().store_id = 2,
            "peer_count" => r.meta.peers.clear(),
            "peer_content" => r.meta.peers[0].id = 2,
            "bucket_increase" | "bucket_precedes_flow" | "epoch_leader_bucket" => {
                r.update_buckets(
                    Some(&RegionBuckets {
                        version: 1,
                        ..Default::default()
                    }),
                    None,
                );
                r.stats.written_bytes = 100;
                if name == "epoch_leader_bucket" {
                    r.meta.epoch.as_mut().unwrap().version += 1;
                    r.leader = None;
                }
            }
            "bucket_disable" => {
                r.update_buckets(None, None);
            }
            "bucket_decrease" => {
                let old = r.report_buckets().cloned();
                r.update_buckets(
                    Some(&RegionBuckets {
                        version: 1,
                        ..Default::default()
                    }),
                    old.as_ref(),
                );
            }
            "write" => r.stats.written_bytes = 100,
            "read" => r.stats.read_bytes = 100,
            "rounded_same" => {
                r.stats.written_bytes = 250;
                origin.stats.written_bytes = 349;
            }
            "divisor_decrease" => r.stats.flow_round_divisor = 10,
            "pending" => r.pending_peers.push(peer.clone()),
            "down" => r.down_peers.push(DownPeer {
                peer: peer.clone(),
                down_seconds: 600,
            }),
            "down_seconds" => r.down_peers[0].down_seconds = 600,
            "pending_role" => r.pending_peers[0].is_witness = true,
            "size" => r.stats.approximate_size = 1,
            "keys" => r.stats.approximate_keys = 1,
            "kv" => r.stats.approximate_kv_size = 1,
            "ia" => r.stats.approximate_ia_kv_size = 1,
            "columnar" => r.stats.approximate_columnar_kv_size = 1,
            "replication" => r.set_replication_status(1, 0),
            "replication_id" => r.set_replication_status(1, 2),
            "unknown_replication" => r.set_replication_status(0, 2),
            "flashback" => r.meta.is_in_flashback = true,
            "flashback_ts" => r.meta.flashback_start_ts = 1,
            "term" => r.set_term(9),
            _ => panic!("unhandled fixture {name}"),
        }
        let result = region_guide(&r, Some(&origin));
        let actual = [
            result.save_kv,
            result.save_cache,
            result.need_sync,
            result.retained,
        ];
        let expected: Vec<bool> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() != 0)
            .collect();
        assert_eq!(actual.as_slice(), expected, "{name}");
        assert_eq!(r.clone_with([]).term(), r.term());
    }
}

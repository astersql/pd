// Copyright 2026 AsterSQL.
use crate::schedule_wire::*;
use astersql_pd_core::{
    KeyRange, Peer, PeerRole, RegionEncryptionMeta, RegionEpoch, RegionInfo, RegionMeta,
};
use astersql_pd_proto::{pdpb, schedulingpb};
use astersql_pd_schedule_hbstream::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use prost::Message;
use serde_json::Value;

fn number(v: &Value, k: &str) -> u64 {
    v[k].as_u64().unwrap_or(0)
}
fn raw(v: &Value, k: &str) -> i32 {
    v[k].as_i64().unwrap_or(0).try_into().unwrap()
}
fn flag(v: &Value, k: &str) -> bool {
    v[k].as_bool().unwrap_or(false)
}
fn bytes(v: &Value, k: &str) -> Vec<u8> {
    STANDARD.decode(v[k].as_str().unwrap_or("")).unwrap()
}
fn optional<T>(v: &Value, k: &str, parse: fn(&Value) -> T) -> Option<T> {
    v.get(k).filter(|x| !x.is_null()).map(parse)
}
fn list<T>(v: &Value, k: &str, parse: fn(&Value) -> T) -> Vec<T> {
    v[k].as_array()
        .map(|xs| xs.iter().map(parse).collect())
        .unwrap_or_default()
}
fn peer(v: &Value) -> Peer {
    Peer {
        id: number(v, "id"),
        store_id: number(v, "store_id"),
        role: match raw(v, "role") {
            0 => PeerRole::Voter,
            1 => PeerRole::Learner,
            2 => PeerRole::IncomingVoter,
            3 => PeerRole::DemotingVoter,
            n => PeerRole::Unknown(n),
        },
        is_witness: flag(v, "is_witness"),
    }
}
fn epoch(v: &Value) -> RegionEpoch {
    RegionEpoch {
        conf_ver: number(v, "conf_ver"),
        version: number(v, "version"),
    }
}
fn region(v: &Value) -> RegionMeta {
    RegionMeta {
        id: number(v, "id"),
        key_range: KeyRange {
            start_key: bytes(v, "start_key"),
            end_key: bytes(v, "end_key"),
        },
        epoch: optional(v, "region_epoch", epoch),
        peers: list(v, "peers", peer),
        encryption_meta: optional(v, "encryption_meta", |x| RegionEncryptionMeta {
            key_id: number(x, "key_id"),
            iv: bytes(x, "iv"),
        }),
        is_in_flashback: flag(v, "is_in_flashback"),
        flashback_start_ts: number(v, "flashback_start_ts"),
    }
}
fn change(v: &Value) -> ChangePeer {
    ChangePeer {
        peer: optional(v, "peer", peer),
        change_type: ChangeType::from_raw(raw(v, "change_type")),
    }
}
fn operation(v: &Value) -> Operation {
    Operation {
        change_peer: optional(v, "change_peer", change),
        transfer_leader: optional(v, "transfer_leader", |x| TransferLeader {
            peer: optional(x, "peer", peer),
            peers: list(x, "peers", |p| (!p.is_null()).then(|| peer(p))),
        }),
        merge: optional(v, "merge", |x| Merge {
            target: optional(x, "target", region),
        }),
        split_region: optional(v, "split_region", |x| SplitRegion {
            policy: SplitPolicy::from_raw(raw(x, "policy")),
            keys: list(x, "keys", |k| {
                STANDARD.decode(k.as_str().unwrap_or("")).unwrap()
            }),
        }),
        change_peer_v2: optional(v, "change_peer_v2", |x| ChangePeerV2 {
            changes: list(x, "changes", |p| (!p.is_null()).then(|| change(p))),
        }),
        switch_witnesses: optional(v, "switch_witnesses", |x| BatchSwitchWitness {
            switch_witnesses: list(x, "switch_witnesses", |p| {
                (!p.is_null()).then(|| SwitchWitness {
                    peer_id: number(p, "peer_id"),
                    is_witness: flag(p, "is_witness"),
                })
            }),
        }),
        change_split: optional(v, "change_split", |x| ChangeSplit {
            auto_split_enabled: flag(x, "auto_split_enabled"),
        }),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
#[test]
fn task_134_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../docs/rust/schedule-execution/fixtures/task_134.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 184);
    for c in cases {
        let input = &c["input"];
        let command = HeartbeatCommand::for_region(
            number(input, "cluster_id"),
            &RegionInfo::new(
                RegionMeta {
                    id: number(input, "region_id"),
                    epoch: optional(input, "region_epoch", epoch),
                    ..Default::default()
                },
                optional(input, "target_peer", peer),
                vec![],
                vec![],
            ),
            operation(&input["operation"]),
        );
        let Some(command) = command else {
            assert_eq!(c["expected"], "suppressed");
            continue;
        };
        let before = command.clone();
        if let Some(path) = c["error"].as_str() {
            let index = c["index"].as_u64().unwrap() as usize;
            let a = to_pdpb(&command).unwrap_err();
            assert_eq!(a.field, path);
            assert_eq!(a.index, index);
            assert_eq!(to_scheduling(&command).unwrap_err(), a);
            assert!(!a.to_string().is_empty());
        } else {
            let p = to_pdpb(&command).unwrap();
            let s = to_scheduling(&command).unwrap();
            let pb = p.encode_to_vec();
            let sb = s.encode_to_vec();
            assert_eq!(hex(&pb), c["pdpb_hex"].as_str().unwrap(), "PD {}", c["id"]);
            assert_eq!(
                hex(&sb),
                c["schedulingpb_hex"].as_str().unwrap(),
                "Scheduling {}",
                c["id"]
            );
            assert_eq!(
                pdpb::RegionHeartbeatResponse::decode(pb.as_slice()).unwrap(),
                p
            );
            assert_eq!(
                schedulingpb::RegionHeartbeatResponse::decode(sb.as_slice()).unwrap(),
                s
            );
        }
        assert_eq!(command, before);
    }
    println!("{} source-derived cases passed", cases.len());
}
#[test]
fn task_134_headers_errors_and_decode_faults() {
    for cluster in [0, 123, u64::MAX] {
        let p = keepalive_pdpb(cluster);
        let s = keepalive_scheduling(cluster);
        assert_eq!(p.header.as_ref().unwrap().cluster_id, cluster);
        assert_eq!(s.header.as_ref().unwrap().cluster_id, cluster);
        assert!(p.header.as_ref().unwrap().error.is_none());
        assert_eq!(
            pdpb::RegionHeartbeatResponse::decode(p.encode_to_vec().as_slice()).unwrap(),
            p
        );
        assert_eq!(
            schedulingpb::RegionHeartbeatResponse::decode(s.encode_to_vec().as_slice()).unwrap(),
            s
        );
        for kind in [0, 1, 5, 37, -1, i32::MIN] {
            for target in [
                None,
                Some(Peer::default()),
                Some(Peer {
                    id: u64::MAX,
                    store_id: 42,
                    role: PeerRole::Unknown(99),
                    is_witness: true,
                }),
            ] {
                let p = error_pdpb(cluster, kind, "fault\0消息".into(), target.clone());
                let header = p.header.as_ref().unwrap();
                assert_eq!(header.cluster_id, cluster);
                let error = header.error.as_ref().unwrap();
                assert_eq!(error.r#type, kind);
                assert_eq!(error.message, "fault\0消息");
                assert_eq!(
                    p.target_peer,
                    target.map(crate::core_region_adapter::peer_to_proto)
                );
                assert_eq!(p.region_id, 0);
                assert!(p.region_epoch.is_none());
                assert!(p.change_peer.is_none());
                assert_eq!(
                    pdpb::RegionHeartbeatResponse::decode(p.encode_to_vec().as_slice()).unwrap(),
                    p
                );
            }
        }
    }
    for bad in [&[0x0a, 0x80][..], &[0x32, 0x01, 0xff], &[0x00]] {
        assert!(pdpb::RegionHeartbeatResponse::decode(bad).is_err());
        assert!(schedulingpb::RegionHeartbeatResponse::decode(bad).is_err());
    }
    assert_eq!(
        hex(&error_pdpb(123, 4, "x".into(), None).encode_to_vec()),
        "0a09087b12050804120178"
    );
    assert_eq!(hex(&keepalive_pdpb(0).encode_to_vec()), "0a00");
    assert_eq!(hex(&keepalive_scheduling(0).encode_to_vec()), "0a00");
    println!("54 error envelopes, 6 keepalives, 6 malformed decodes passed");
}

// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    KeyRange, Peer, PeerRole, RegionEncryptionMeta, RegionEpoch, RegionInfo, RegionMeta,
};
use base64::{Engine, engine::general_purpose::STANDARD};
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

#[test]
fn task_012_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_012.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 34);
    for c in cases {
        let meta = RegionMeta {
            id: number(c, "region_id"),
            epoch: optional(c, "region_epoch", epoch),
            ..Default::default()
        };
        let source = RegionInfo::new(meta, optional(c, "target_peer", peer), vec![], vec![]);
        let op = operation(&c["operation"]);
        assert_eq!(
            op.is_empty(),
            c["operation"].is_null() || c["operation"].as_object().unwrap().is_empty(),
            "{}",
            c["id"]
        );
        let actual = HeartbeatCommand::for_region(123, &source, op);
        let output = &c["output"];
        let expected = (!output.is_null()).then(|| HeartbeatCommand {
            cluster_id: number(&output["header"], "cluster_id"),
            region_id: number(output, "region_id"),
            region_epoch: optional(output, "region_epoch", epoch),
            target_peer: peer(&output["target_peer"]),
            operation: operation(output),
        });
        assert_eq!(actual, expected, "{}", c["id"]);
    }
    println!("34 frozen Go cases passed; 0 failed");
}

#[test]
fn task_012_owned_snapshot_and_enum_domain() {
    for n in [i32::MIN, -1, 0, 1, 2, 3, 37, i32::MAX] {
        assert_eq!(ChangeType::from_raw(n).as_raw(), n);
        assert_eq!(SplitPolicy::from_raw(n).as_raw(), n);
    }
    let meta = RegionMeta {
        id: u64::MAX,
        epoch: Some(RegionEpoch {
            conf_ver: 5,
            version: 7,
        }),
        ..Default::default()
    };
    let source = RegionInfo::new(
        meta,
        Some(Peer {
            id: 11,
            store_id: 22,
            ..Default::default()
        }),
        vec![],
        vec![],
    );
    let mut op = Operation {
        split_region: Some(SplitRegion {
            policy: SplitPolicy::UseKey,
            keys: vec![vec![0, 255], vec![]],
        }),
        ..Default::default()
    };
    let command = HeartbeatCommand::for_region(u64::MAX, &source, op.clone()).unwrap();
    op.split_region.as_mut().unwrap().keys[0][0] = 9;
    let mut cloned = command.clone();
    cloned.operation.split_region.as_mut().unwrap().keys[0][0] = 8;
    assert_eq!(
        command.operation.split_region.unwrap().keys,
        vec![vec![0, 255], vec![]]
    );
    assert_eq!(command.region_id, u64::MAX);
    assert_eq!(command.cluster_id, u64::MAX);
    assert_eq!(
        source.meta().epoch,
        Some(RegionEpoch {
            conf_ver: 5,
            version: 7
        })
    );
    // Missing leader is a normal suppressed send, including zero/default metadata.
    let no_leader = RegionInfo::new(RegionMeta::default(), None, vec![], vec![]);
    assert!(HeartbeatCommand::for_region(0, &no_leader, Operation::default()).is_none());
    let source = RegionInfo::new(RegionMeta::default(), Some(Peer::default()), vec![], vec![]);
    let command = HeartbeatCommand::for_region(0, &source, Operation::default()).unwrap();
    assert_eq!(command.region_epoch, None);
    assert_eq!(command.target_peer, Peer::default());
    assert!(command.operation.is_empty());
}

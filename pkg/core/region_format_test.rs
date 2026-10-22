// Copyright 2026 AsterSQL.
use crate::*;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("rust-fixtures/core_task_36.json")).unwrap()
}

#[test]
fn core_task_36_rule_codec() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("rust-fixtures/core_task_36.json")).unwrap();
    for row in fixture["rules"].as_array().unwrap() {
        let rule = Rule::new(row[0].as_str().unwrap());
        assert_eq!(rule.to_string(), row[1].as_str().unwrap());
        assert_eq!(
            rule.marshal_json(),
            format!("{:?}", row[1].as_str().unwrap()).as_bytes()
        );
        assert_eq!(Rule::unmarshal_json(&rule.marshal_json()), rule);
    }
    for raw in [
        b"null".as_slice(),
        b"123",
        b" \"leader-scatter\" ",
        b"invalid",
    ] {
        assert_eq!(Rule::unmarshal_json(raw), Rule::Unknown);
    }
}
#[test]
fn core_task_36_keys_and_meta() {
    let fixture = fixture();
    let row = &fixture["scenarios"][0];
    let bytes: Vec<u8> = row["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();
    assert_eq!(
        hex_region_key(&bytes),
        row["expected"].as_str().unwrap().as_bytes()
    );
    assert_eq!(hex_region_key_str(&[]), "");
    let mut a = RegionMeta::default();
    let mut b = a.clone();
    b.key_range.start_key = vec![0xab];
    assert_eq!(
        diff_region_key_info(&a, &b),
        "StartKey Changed:{} -> {AB}, EndKey:{}"
    );
    assert_eq!(diff_region_key_info(&a, &a), "StartKey:{}, EndKey:{}");
    a.id = 7;
    a.key_range.end_key = vec![255];
    a.epoch = Some(RegionEpoch {
        conf_ver: 2,
        version: 3,
    });
    a.peers.push(Peer {
        id: 1,
        store_id: 2,
        role: PeerRole::Learner,
        is_witness: true,
    });
    a.is_in_flashback = true;
    a.flashback_start_ts = 9;
    let before = a.clone();
    assert_eq!(
        region_to_hex_meta(Some(&a)).to_string(),
        "id:7 end_key:\"FF\" region_epoch:<conf_ver:2 version:3 > peers:<id:1 store_id:2 role:Learner is_witness:true > is_in_flashback:true flashback_start_ts:9"
    );
    assert_eq!(a, before);
    assert!(std::panic::catch_unwind(|| region_to_hex_meta(None).to_string()).is_err());
    assert_eq!(regions_to_hex_meta(&[]).to_string(), "");
    assert_eq!(
        regions_to_hex_meta(&[a.clone(), a.clone()]).to_string(),
        format!(
            "{} {} ",
            region_to_hex_meta(Some(&a)),
            region_to_hex_meta(Some(&a))
        )
        .trim()
    );
    b.key_range.end_key = vec![0];
    assert!(diff_region_key_info(&a, &b).contains("EndKey Changed:{FF} -> {00}"));
}
#[test]
fn core_task_36_peer_diffs() {
    let a = RegionMeta {
        peers: vec![Peer {
            id: 1,
            store_id: 2,
            ..Peer::default()
        }],
        ..RegionMeta::default()
    };
    let mut b = a.clone();
    assert_eq!(diff_region_peers_info(&a, &b), "");
    b.peers[0].role = PeerRole::Unknown(99);
    assert_eq!(
        diff_region_peers_info(&a, &b),
        "Remove peer:{id:1 store_id:2 },Add peer:{id:1 store_id:2 role:99 }"
    );
    b.peers.clear();
    assert_eq!(
        diff_region_peers_info(&a, &b),
        "Remove peer:{id:1 store_id:2 }"
    );
    assert_eq!(
        diff_region_peers_info(&b, &a),
        "Add peer:{id:1 store_id:2 }"
    );
}

#[test]
fn core_task_36_peer_order_duplicates_and_roles() {
    let a = RegionMeta {
        peers: vec![
            Peer::default(),
            Peer {
                id: 9,
                ..Peer::default()
            },
        ],
        ..RegionMeta::default()
    };
    let mut b = a.clone();
    b.peers.reverse();
    assert_eq!(diff_region_peers_info(&a, &b), "");
    b.peers.push(Peer::default());
    assert_eq!(diff_region_peers_info(&a, &b), "");
    for (role, text) in [
        (PeerRole::IncomingVoter, "IncomingVoter"),
        (PeerRole::DemotingVoter, "DemotingVoter"),
    ] {
        let b = RegionMeta {
            peers: vec![Peer {
                role,
                ..Peer::default()
            }],
            epoch: Some(RegionEpoch::default()),
            ..RegionMeta::default()
        };
        assert_eq!(
            region_to_hex_meta(Some(&b)).to_string(),
            format!("region_epoch:<> peers:<role:{text} >")
        );
    }
}

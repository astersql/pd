// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_codec::{encode_bytes, encode_keyspace_boundary as boundary, keyspace_modes};
use serde_json::json;
use std::sync::Arc;
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn rule(id: u32, modes: &[u8]) -> LabelRule {
    LabelRule::from_json(&serde_json::to_vec(&json!({
        "id":format!("keyspaces/{id}"),"labels":[{"key":"id","value":id.to_string()}],"rule_type":"key-range",
        "data":modes.iter().map(|m|json!({"start_key":hex(&boundary(*m,id)),"end_key":hex(&boundary(*m,id+1))})).collect::<Vec<_>>()
    })).unwrap(),0).unwrap()
}
#[test]
fn label_keyspace_canonical_shape_and_ids() {
    for (text, want) in [
        ("0", Some(0)),
        ("42", Some(42)),
        ("16777215", Some(MAX_KEYSPACE_ID)),
        ("", None),
        ("01", None),
        ("+1", None),
        ("-1", None),
        ("16777216", None),
        ("4294967296", None),
        ("１", None),
    ] {
        assert_eq!(
            parse_keyspace_rule_id(&format!("keyspaces/{text}")),
            want,
            "{text}"
        );
    }
    for kind in 0..9 {
        let mut r = rule(42, &keyspace_modes());
        match kind {
            0 => r.index = 1,
            1 => r.labels[0].ttl = "1h".into(),
            2 => r.labels[0].start_at = "ignored".into(),
            3 => r.labels[0].key = "other".into(),
            4 => r.labels[0].value = "042".into(),
            5 => r.id = "keyspaces/042".into(),
            6 => r.labels.push(r.labels[0].clone()),
            7 => {
                r.data = json!([{"start_key":hex(&boundary(b'x',42)),"end_key":hex(&boundary(b'x',43))},{"start_key":hex(&boundary(b'x',42)),"end_key":hex(&boundary(b'x',43))}])
            }
            _ => r.data = json!([{"start_key":"78","end_key":"79"}]),
        }
        r.check_and_adjust(0).unwrap();
        assert!(
            !KeyspaceRuleIndex::default().add(Arc::new(r)),
            "noncanonical kind {kind}"
        );
    }
}
#[test]
fn label_keyspace_ownership_and_atomic_replace() {
    let mut i = KeyspaceRuleIndex::default();
    let r = Arc::new(rule(42, &keyspace_modes()));
    assert!(i.add(r.clone()));
    assert!(i.contains(&r));
    assert!(!i.add(r.clone()));
    let equal = Arc::new(rule(42, &keyspace_modes()));
    assert!(!i.contains(&equal));
    assert!(!i.remove(&r.id, &equal));
    let txn = Arc::new(rule(43, b"x"));
    assert!(i.add(txn.clone()));
    let both = Arc::new(rule(43, &keyspace_modes()));
    assert!(!i.add(both.clone()));
    assert!(
        i.get_rule(&boundary(b'r', 43), &boundary(b'r', 44))
            .is_none()
    );
    let raw_owner = Arc::new(rule(43, b"r"));
    assert!(i.add(raw_owner.clone()));
    // old owns raw43, while the requested txn43 belongs to a different object.
    assert!(!i.replace(&raw_owner, both.clone()));
    assert!(i.contains(&raw_owner));
    assert!(i.contains(&txn));
    assert!(i.remove(&raw_owner.id, &raw_owner));
    assert!(!i.replace(&r, both.clone()));
    assert!(i.contains(&r));
    assert!(i.contains(&txn));
    let next = Arc::new(rule(42, b"x"));
    assert!(i.replace(&r, next.clone()));
    assert!(!i.contains(&r));
    assert!(i.contains(&next));
    assert!(
        i.get_rule(&boundary(b'r', 42), &boundary(b'r', 43))
            .is_none()
    );
    assert!(!i.remove(&r.id, &r));
    assert!(i.remove(&next.id, &next));
    assert!(i.remove(&txn.id, &txn));
    assert!(i.is_empty());
}
#[test]
fn label_keyspace_boundaries_and_incomplete_prefixes() {
    let mut i = KeyspaceRuleIndex::default();
    let ids = [0, 63, 64, 255, 256, 1023, 1024, 65535, MAX_KEYSPACE_ID];
    for id in ids {
        assert!(i.add(Arc::new(rule(id, &keyspace_modes()))));
    }
    let mut expected = Vec::new();
    for m in keyspace_modes() {
        for id in ids {
            expected.push(boundary(m, id));
            expected.push(boundary(m, id + 1));
        }
    }
    expected.sort();
    expected.dedup();
    assert_eq!(
        i.get_split_keys(&[], &[]),
        expected.iter().map(|b| b.as_slice()).collect::<Vec<_>>()
    );
    for mode in keyspace_modes() {
        for id in ids {
            let start = boundary(mode, id);
            let end = boundary(mode, id + 1);
            assert_eq!(
                i.get_rule(&start, &end).unwrap().id,
                format!("keyspaces/{id}")
            );
            assert!(!i.has_split_key(&start, &end));
            let payload = encode_bytes(&[mode, (id >> 16) as u8, (id >> 8) as u8, id as u8, b'a']);
            assert!(i.get_rule(&payload, &end).is_some());
        }
    }
    let b = boundary(b'x', 256);
    let mut below = b;
    below[8] -= 1;
    assert_eq!(i.match_rule(&below, &b).0.unwrap().id, "keyspaces/255");
    assert_eq!(i.match_rule(&b[..4], &b).0.unwrap().id, "keyspaces/255");
    let inside = encode_bytes(&[b'x', 0, 1, 0, b'a']);
    assert!(matches!(i.match_rule(&below, &inside), (None, false)));
    let mut malformed = b;
    malformed[4] = 1;
    malformed[8] -= 1;
    assert_eq!(
        i.match_rule(&malformed, &boundary(b'x', 257)).0.unwrap().id,
        "keyspaces/256"
    );
    for m in keyspace_modes() {
        let fence = boundary(m, MAX_KEYSPACE_ID + 1);
        assert_eq!(
            i.match_rule(&fence[..1], &fence).0.unwrap().id,
            "keyspaces/16777215"
        );
    }
    assert!(matches!(i.match_rule(&b, &[]), (None, false)));
    assert!(!i.has_split_key(b"z", &[]));
    let sparse = {
        let mut s = KeyspaceRuleIndex::default();
        s.add(Arc::new(rule(MAX_KEYSPACE_ID, b"x")));
        s
    };
    assert!(matches!(
        sparse.match_rule(&boundary(b'x', 1), &boundary(b'x', 2)),
        (None, true)
    ));
    assert!(!sparse.has_split_key(&boundary(b'x', 0), &boundary(b'x', MAX_KEYSPACE_ID)));
}
#[test]
fn label_keyspace_generic_equivalence_and_transitions() {
    let mut r = rule(42, &keyspace_modes());
    let mut index = LabelRuleIndex::new([r.clone(), rule(43, b"x")]);
    let all = index
        .get_split_keys(&[], &[])
        .iter()
        .map(|b| b.to_vec())
        .collect::<Vec<_>>();
    assert_eq!(all.len(), 5);
    for m in keyspace_modes() {
        assert_eq!(
            index
                .get_range_rules(&boundary(m, 42), &boundary(m, 43))
                .unwrap()
                .len(),
            1
        );
    }
    assert!(
        index
            .get_range_rules(&boundary(b'x', 42), &boundary(b'x', 44))
            .is_none()
    );
    r = rule(42, b"x");
    index.set(r.clone());
    assert!(
        index
            .get_range_rules(&boundary(b'r', 42), &boundary(b'r', 43))
            .unwrap()
            .is_empty()
    );
    r.labels[0].key = "fallback".into();
    index.set(r.clone());
    assert_eq!(
        index
            .get_range_rules(&boundary(b'x', 42), &boundary(b'x', 43))
            .unwrap()[0]
            .labels[0]
            .key,
        "fallback"
    );
    index.set(rule(42, &keyspace_modes()));
    index.delete("keyspaces/42");
    assert_eq!(
        index.get_split_keys(&[], &[]),
        vec![boundary(b'x', 43).as_slice(), boundary(b'x', 44).as_slice()]
    );
    let a = [1u8];
    let b = [2u8];
    let c = [3u8];
    assert_eq!(
        merge_split_keys(vec![&a, &b], vec![&b, &c]),
        vec![&a[..], &b[..], &c[..]]
    );
}

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn label_keyspace_actual_go_runtime_fixture() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/labeler/task16/go.json"
    ))
    .unwrap();
    let rules: Vec<_> = oracle["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| LabelRule::from_json(&serde_json::to_vec(r).unwrap(), 0).unwrap())
        .collect();
    let index = LabelRuleIndex::new(rules.clone());
    let mut ks = KeyspaceRuleIndex::default();
    let owned: Vec<_> = rules.into_iter().map(Arc::new).collect();
    for rule in &owned {
        ks.add(rule.clone());
    }
    let actual: Vec<_> = index
        .get_split_keys(&[], &[])
        .into_iter()
        .map(hex)
        .collect();
    assert_eq!(serde_json::to_value(actual).unwrap(), oracle["splits"]);
    for q in oracle["queries"].as_array().unwrap() {
        let start = unhex(q["start"].as_str().unwrap());
        let end = unhex(q["end"].as_str().unwrap());
        let (mut ids, within) = if q["keyspace_only"] == true {
            let (r, w) = ks.match_rule(&start, &end);
            (r.into_iter().map(|r| r.id.clone()).collect::<Vec<_>>(), w)
        } else {
            let r = index.get_range_rules(&start, &end);
            (
                r.as_ref()
                    .into_iter()
                    .flatten()
                    .map(|r| r.id.clone())
                    .collect(),
                r.is_some(),
            )
        };
        ids.sort();
        assert_eq!(serde_json::to_value(ids).unwrap(), q["ids"], "{q}");
        assert_eq!(within, q["within"].as_bool().unwrap(), "{q}");
    }
    let old = &owned[0];
    let mut probe = (*owned[6]).clone();
    probe.id = old.id.clone();
    let probe = Arc::new(probe);
    let mut operations = vec![
        ks.contains(old),
        ks.contains(&owned[5]),
        ks.add(old.clone()),
        ks.replace(old, probe.clone()),
        ks.contains(&probe),
        ks.remove(&old.id, old),
        ks.remove(&probe.id, &probe),
    ];
    let mut noncanonical = (*probe).clone();
    noncanonical.index = 1;
    operations.push(ks.add(Arc::new(noncanonical)));
    operations.push(ks.add(old.clone()));
    let expected: Vec<_> = oracle["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["value"].as_bool().unwrap())
        .collect();
    assert_eq!(operations, expected);
}
#[test]
fn label_keyspace_legacy_range_equivalence() {
    use astersql_pd_schedule_rangelist::Builder;
    let mut rules: Vec<_> = [0, 1, 63, 64, 255, 256, 1023, 1024, 65535, MAX_KEYSPACE_ID]
        .into_iter()
        .map(|id| rule(id, &keyspace_modes()))
        .collect();
    let mut overlay = rule(64, b"x");
    overlay.id = "overlay".into();
    overlay.index = 2;
    rules.push(overlay);
    let generic=LabelRule::from_json(br#"{"id":"all","labels":[{"key":"all","value":"yes"}],"rule_type":"key-range","data":[{"start_key":"","end_key":""}]}"#,0).unwrap();
    rules.push(generic);
    let mut b = Builder::new();
    for r in &rules {
        for range in r.key_ranges().unwrap() {
            b.add_item(&range.start_key, &range.end_key, r.id.clone());
        }
    }
    let legacy = b.build();
    let mut index = LabelRuleIndex::new(rules);
    let mut keys = vec![vec![], b"a".to_vec(), b"z".to_vec()];
    for mode in keyspace_modes() {
        for id in [
            0,
            1,
            63,
            64,
            255,
            256,
            1023,
            1024,
            65535,
            MAX_KEYSPACE_ID,
            MAX_KEYSPACE_ID + 1,
        ] {
            let k = boundary(mode, id);
            keys.push(k.to_vec());
            for length in [1, 3, 4, 8] {
                keys.push(k[..length].to_vec());
            }
            let mut before = k;
            before[8] -= 1;
            keys.push(before.to_vec());
            let mut after = k;
            after[8] += 1;
            keys.push(after.to_vec());
        }
    }
    keys.sort();
    keys.dedup();
    for start in &keys {
        for end in keys.iter().filter(|end| end.is_empty() || *end > start) {
            assert_eq!(
                index.get_split_keys(start, end),
                legacy.get_split_keys(start, end),
                "splits {start:x?}..{end:x?}"
            );
            let (data, within) = legacy.get_data_by_range(start, end);
            let actual = index.get_range_rules(start, end);
            assert_eq!(actual.is_some(), within, "within {start:x?}..{end:x?}");
            if within {
                let mut expected = data.unwrap_or_default().to_vec();
                expected.sort();
                let mut actual: Vec<_> =
                    actual.unwrap().into_iter().map(|r| r.id.clone()).collect();
                actual.sort();
                assert_eq!(actual, expected, "rules {start:x?}..{end:x?}");
            }
        }
    }
    assert!(!index.remove_expired_labels("keyspaces/64", i128::MAX));
    index.delete("keyspaces/64");
    assert!(
        !index
            .get_range_rules(&boundary(b'x', 64), &boundary(b'x', 65))
            .unwrap()
            .iter()
            .any(|r| r.id == "keyspaces/64")
    );
    let snapshot = LabelRuleIndex::new([rule(42, b"x"), rule(42, b"r")]);
    assert_eq!(snapshot.rules().count(), 1);
    assert_eq!(
        snapshot.get_split_keys(&[], &[]),
        vec![boundary(b'r', 42).as_slice(), boundary(b'r', 43).as_slice()]
    );
}

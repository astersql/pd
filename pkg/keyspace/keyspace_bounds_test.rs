// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn mcs_025_behavior() {
    let b = make_region_bound(0x00ff_ffff);
    assert_eq!(b.raw_left_bound, [b'r', 255, 255, 255, 0, 0, 0, 0, 251]);
    assert_eq!(b.raw_right_bound, [b's', 0, 0, 0, 0, 0, 0, 0, 251]);
    assert_eq!(b.txn_right_bound, [b'y', 0, 0, 0, 0, 0, 0, 0, 251]);
}
#[test]
fn mcs_025_kernel_protection_name_limits_and_id_errors() {
    use astersql_pd_keyspace_constant::*;
    for kernel in [Kernel::Classic, Kernel::NextGeneration] {
        for id in [0, 1, 100, SYSTEM_KEYSPACE_ID, MAX_VALID_KEYSPACE_ID] {
            assert_eq!(kernel.validate_id(id).is_err(), kernel.is_protected_id(id));
        }
        for id in [MAX_VALID_KEYSPACE_ID + 1, u32::MAX] {
            assert_eq!(
                kernel.validate_id(id).unwrap_err().to_string(),
                format!(
                    "illegal keyspace id {id}, larger than spaceID Max {MAX_VALID_KEYSPACE_ID}"
                )
            );
        }
        for name in [
            "keyspaceName1",
            "keyspace_name_1",
            "10",
            "789z-_",
            "18446744073709551615",
            "scope1",
            "-----",
        ] {
            kernel.validate_name(name).unwrap();
        }
        for name in [
            "",
            "keyspace/",
            "keyspace:1",
            "many many spaces",
            "keyspace?limit=1",
            "keyspace%1",
            "789z-_)",
            "18446744073709551615a",
            "中",
            "é",
        ] {
            assert_eq!(
                kernel.validate_name(name).unwrap_err().to_string(),
                format!(
                    "illegal keyspace name {name}, should contain only alphanumerical and underline"
                )
            );
        }
        for name in [DEFAULT_KEYSPACE_NAME, SYSTEM_KEYSPACE_NAME] {
            assert_eq!(
                kernel.validate_name(name).is_err(),
                kernel.is_protected_name(name)
            );
        }
    }
    assert_eq!(mask_keyspace_id(u32::MAX), 255);
    assert_eq!(mask_keyspace_id(256), 0);
}
#[test]
fn mcs_025_label_identity_is_less_strict_than_fast_index_shape() {
    use astersql_pd_schedule_labeler::RegionLabel;
    let mut r = make_txn_label_rule(1);
    assert_eq!(parse_keyspace_id_from_label_rule(Some(&r)), Some(1));
    r.index = 99;
    r.rule_type = "other".into();
    r.data = serde_json::Value::Null;
    r.labels[0].value = "0001".into();
    r.labels[0].ttl = "1h".into();
    assert_eq!(parse_keyspace_id_from_label_rule(Some(&r)), Some(1));
    let mut second = RegionLabel::default();
    second.key = "id".into();
    second.value = "2".into();
    r.labels.push(second);
    assert_eq!(
        parse_keyspace_id_from_label_rule(Some(&r)),
        Some(1),
        "first id label wins"
    );
    for suffix in ["", "01", "+1", "-1", "1a", "16777216", "4294967296", "１"] {
        r.id = format!("keyspaces/{suffix}");
        assert_eq!(parse_keyspace_id_from_label_rule(Some(&r)), None);
    }
    r.id = "keyspaces/1".into();
    for value in ["", "+1", "-1", " 1", "1 ", "4294967296", "2"] {
        r.labels[0].value = value.into();
        assert_eq!(parse_keyspace_id_from_label_rule(Some(&r)), None);
    }
    r.labels.clear();
    r.id = "keyspaces/0".into();
    assert_eq!(parse_keyspace_id_from_label_rule(Some(&r)), None);
    assert_eq!(parse_keyspace_id_from_label_rule(None), None);
}
#[test]
fn mcs_025_full_decoding_rejects_padding_and_accepts_only_raw_txn_prefixes() {
    use astersql_pd_codec::{encode_bytes, make_keyspace_prefix};
    assert_eq!(extract_keyspace_id(&[]), Some((0xffffff, BoundType::Txn)));
    for mode in [BoundType::Raw, BoundType::Txn] {
        for id in [0, 1, 4242, 0x0102ff, 0xffffff] {
            let b = make_region_bound(id);
            let (left, _) = mode.bounds(&b);
            assert_eq!(extract_keyspace_id(left), Some((id, mode)));
            let mut extra = left.to_vec();
            extra.push(0xde);
            assert_eq!(
                extract_keyspace_id(&extra),
                Some((id, mode)),
                "DecodeBytes remainder is intentionally ignored"
            );
        }
    }
    let mut raw = make_keyspace_prefix(b'x', 42).to_vec();
    raw.extend_from_slice(b"payload-longer-than-one-group");
    assert_eq!(
        extract_keyspace_id(&encode_bytes(&raw)),
        Some((42, BoundType::Txn))
    );
    for raw in [vec![b't', 1, 2, 3], vec![b'x']] {
        assert_eq!(extract_keyspace_id(&encode_bytes(&raw)), None);
    }
    let mut corrupt = make_region_bound(42).txn_left_bound;
    corrupt[7] = 1;
    assert_eq!(extract_keyspace_id(&corrupt), None);
    for n in 1..9 {
        assert_eq!(extract_keyspace_id(&corrupt[..n]), None);
    }
    assert_eq!(
        extract_keyspace_id(&make_region_bound(0xffffff).txn_right_bound),
        None
    );
}

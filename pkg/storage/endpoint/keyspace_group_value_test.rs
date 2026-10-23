// Copyright 2026 AsterSQL.

use super::{KeyspaceGroupMember, UserKind};

#[test]
fn keyspace_group_value_user_kinds() {
    for (n, name) in ["basic", "standard", "enterprise"].into_iter().enumerate() {
        let kind = UserKind(n as i64);
        assert_eq!(kind.to_string(), name);
        assert_eq!(UserKind::from_name(name), kind);
        assert!(UserKind::is_valid(name));
        assert_eq!(serde_json::to_string(&kind).unwrap(), n.to_string());
    }
    assert_eq!(UserKind::default(), UserKind::BASIC);
    for name in [
        "",
        "Basic",
        "STANDARD",
        "Enterprise",
        "unknown",
        " basic",
        "basic ",
        "0",
    ] {
        assert_eq!(UserKind::from_name(name), UserKind::BASIC);
        assert!(!UserKind::is_valid(name));
    }
    for kind in [UserKind(-1), UserKind::COUNT, UserKind(i64::MAX)] {
        assert_eq!(kind.to_string(), "unknown UserKind");
        assert_eq!(
            serde_json::from_str::<UserKind>(&kind.0.to_string()).unwrap(),
            kind
        );
    }
}

#[test]
fn keyspace_group_value_addresses() {
    for (a, b, expected) in [
        ("", "", true),
        ("http://127.0.0.1", "https://127.0.0.1", true),
        ("127.0.0.1", "http://127.0.0.1", true),
        ("https://[::1]:123/a?b=c", "http://[::1]:123/a?b=c", true),
        ("http://host", "http://host/", false),
        ("http://host:80", "https://host:443", false),
        ("HTTP://host", "host", false),
        ("http://HOST", "https://host", false),
        ("ftp://host", "host", false),
        ("https://http://host", "host", true),
        ("http://https://host", "host", false),
    ] {
        let member = KeyspaceGroupMember {
            address: a.into(),
            priority: -1,
        };
        assert_eq!(member.is_address_equivalent(b), expected, "{a:?} vs {b:?}");
        let reverse = KeyspaceGroupMember {
            address: b.into(),
            ..member
        };
        assert_eq!(reverse.is_address_equivalent(a), expected);
    }
}

#[test]
fn keyspace_group_value_member_json() {
    // Actual Go SaveKeyspaceGroup bytes, captured from etcd revision 2,
    // TestKeyspaceGroupReconcileWatcherTracksExternalChanges; member substring.
    let go = r#"{"address":"http://initial-tso","priority":0}"#;
    let member = KeyspaceGroupMember {
        address: "http://initial-tso".into(),
        priority: 0,
    };
    assert_eq!(serde_json::to_string(&member).unwrap(), go);
    assert_eq!(
        serde_json::from_str::<KeyspaceGroupMember>(go).unwrap(),
        member
    );
    assert_eq!(
        serde_json::from_str::<KeyspaceGroupMember>("{}").unwrap(),
        KeyspaceGroupMember::default()
    );
    for json in [
        "null",
        r#"{"address":null,"priority":null,"ignored":[1,2]}"#,
    ] {
        assert_eq!(
            serde_json::from_str::<KeyspaceGroupMember>(json).unwrap(),
            KeyspaceGroupMember::default()
        );
    }
    assert_eq!(
        serde_json::from_str::<KeyspaceGroupMember>(
            r#"{"aDdReSs":"http://initial-tso","PRIORITY":1,"priority":0,"address":null}"#
        )
        .unwrap(),
        member
    );
    let negative = KeyspaceGroupMember {
        address: String::new(),
        priority: i64::MIN,
    };
    assert_eq!(
        serde_json::from_str::<KeyspaceGroupMember>(&serde_json::to_string(&negative).unwrap())
            .unwrap(),
        negative
    );
    for bad in [
        r#"{"priority":1.5}"#,
        r#"{"priority":9223372036854775808}"#,
        r#"{"address":1}"#,
    ] {
        assert!(serde_json::from_str::<KeyspaceGroupMember>(bad).is_err());
    }
}

// Copyright 2026 AsterSQL.

use super::{KeyspaceGroup, MergeState, OptionalKeyspaceGroupState, SplitState};

#[test]
fn keyspace_group_state_nil_and_absent() {
    for group in [None, Some(&KeyspaceGroup::default())] {
        assert!(!group.is_splitting());
        assert!(!group.is_split_source());
        assert!(!group.is_split_target());
        assert_eq!(group.split_source(), 0);
        assert!(!group.is_merging());
        assert!(!group.is_merge_source());
        assert!(!group.is_merge_target());
    }
}

#[test]
fn keyspace_group_state_split_roles() {
    for id in [0, 1, u32::MAX] {
        for source in [0, 1, u32::MAX] {
            let group = KeyspaceGroup {
                id,
                split_state: Some(SplitState {
                    split_source: source,
                }),
                ..Default::default()
            };
            assert!(group.is_splitting());
            assert_eq!(group.is_split_source(), source == id);
            assert_eq!(group.is_split_target(), source != id);
            assert_eq!(group.split_source(), source);
            assert_eq!(Some(&group).is_split_source(), source == id);
        }
    }
}

#[test]
fn keyspace_group_state_merge_roles() {
    for id in [0, 2, u32::MAX] {
        for list in [
            None,
            Some(vec![]),
            Some(vec![1, 2, 3]),
            Some(vec![id, id]),
            Some(vec![u32::MAX, 0]),
        ] {
            let source = list.as_ref().is_some_and(|ids| ids.contains(&id));
            let group = KeyspaceGroup {
                id,
                split_state: Some(SplitState { split_source: id }),
                merge_state: Some(MergeState { merge_list: list }),
                ..Default::default()
            };
            assert!(group.is_merging());
            assert_eq!(group.is_merge_source(), source);
            assert_eq!(group.is_merge_target(), !source);
            assert_eq!(Some(&group).is_merge_source(), source);
            assert!(group.is_splitting());
        }
    }
}

#[test]
fn keyspace_group_state_go_split_json() {
    // Actual Go SaveKeyspaceGroup bytes from task 22's read-only etcd capture,
    // TestKeyspaceGroupReconcileWatcherTracksExternalChanges, revision 5.
    let go = r#"{"id":2,"user-kind":"enterprise","split-state":{"split-source":2},"members":[{"address":"http://tso-2","priority":0}],"keyspaces":[42,43]}"#;
    let mut group: KeyspaceGroup = serde_json::from_str(go).unwrap();
    assert_eq!(group.id, 2);
    assert_eq!(group.split_source(), 2);
    assert!(group.is_split_source());
    assert!(!group.is_merging());
    group.keyspace_lookup_table.insert(999);
    assert_eq!(serde_json::to_string(&group).unwrap(), go);
    let decoded: KeyspaceGroup = serde_json::from_str(go).unwrap();
    assert!(decoded.keyspace_lookup_table.is_empty());
}

#[test]
fn keyspace_group_state_json_boundaries() {
    // Additional expected strings below derive from Go tags/encoding/json,
    // rather than claiming additional Go execution samples.
    assert_eq!(
        serde_json::to_string(&KeyspaceGroup::default()).unwrap(),
        r#"{"id":0,"user-kind":"","members":null,"keyspaces":null}"#
    );
    for (input, expected) in [
        (r#"{}"#, r#"{"merge-list":null}"#),
        (r#"{"merge-list":null}"#, r#"{"merge-list":null}"#),
        (r#"{"merge-list":[]}"#, r#"{"merge-list":[]}"#),
        (r#"{"merge-list":[2,3,2]}"#, r#"{"merge-list":[2,3,2]}"#),
    ] {
        let state: MergeState = serde_json::from_str(input).unwrap();
        assert_eq!(serde_json::to_string(&state).unwrap(), expected);
    }
    let group: KeyspaceGroup = serde_json::from_str(
        r#"{"ID":7,"id":null,"split-STATE":{},"merge-state":{"MERGE-LIST":[2,3]},"members":[],"keyspaces":[],"KeyspaceLookupTable":{"9":{}},"unknown":true}"#).unwrap();
    assert_eq!(group.id, 7);
    assert!(group.is_split_target());
    assert_eq!(group.split_source(), 0);
    assert!(group.is_merge_target());
    assert!(group.keyspace_lookup_table.is_empty());
    assert_eq!(
        serde_json::to_string(&group).unwrap(),
        r#"{"id":7,"user-kind":"","split-state":{"split-source":0},"merge-state":{"merge-list":[2,3]},"members":[],"keyspaces":[]}"#
    );
    for json in ["null", "{}", r#"{"split-state":null,"merge-state":null}"#] {
        assert_eq!(
            serde_json::from_str::<KeyspaceGroup>(json).unwrap(),
            KeyspaceGroup::default()
        );
    }
    for bad in [
        r#"{"id":-1}"#,
        r#"{"split-state":{"split-source":4294967296}}"#,
        r#"{"merge-state":{"merge-list":[1.5]}}"#,
        r#"{"merge-state":[]}"#,
        r#"{"keyspaces":[-1]}"#,
        r#"{"user-kind":1}"#,
    ] {
        assert!(serde_json::from_str::<KeyspaceGroup>(bad).is_err(), "{bad}");
    }
}

#[test]
fn keyspace_group_state_repeated_and_null_json() {
    let group: KeyspaceGroup = serde_json::from_str(r#"{"id":7,"split-state":{"split-source":7},"split-state":{},"merge-state":{"merge-list":[7,null]},"merge-state":{},"keyspaces":[null,42]}"#).unwrap();
    assert!(group.is_split_source());
    assert!(group.is_merge_source());
    assert_eq!(group.merge_state.unwrap().merge_list, Some(vec![7, 0]));
    assert_eq!(group.keyspaces, Some(vec![0, 42]));
    let group: KeyspaceGroup = serde_json::from_str(r#"{"split-state":{"split-source":7},"split-state":null,"split-state":{},"merge-state":{"merge-list":[7]},"merge-state":null,"merge-state":{}}"#).unwrap();
    assert_eq!(group.split_source(), 0);
    assert!(group.is_split_source());
    assert!(group.is_merge_target());
    assert_eq!(group.merge_state.unwrap().merge_list, None);
}

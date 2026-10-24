// Copyright 2026 AsterSQL.
use crate::{group::*, parse::*};
fn raw(s: &[&str]) -> Vec<String> {
    s.iter().map(|s| s.to_string()).collect()
}
#[test]
fn affinity_values_ids_and_queries() {
    assert_eq!(parse_group_ids(&[]).unwrap(), None);
    assert_eq!(
        parse_group_ids(&raw(&["  a ", "a", "", "\u{0085}B_1-\u{3000}"])).unwrap(),
        Some(raw(&["a", "B_1-"]))
    );
    assert_eq!(parse_group_ids(&raw(&[" "])).unwrap(), Some(vec![]));
    for id in ["", "a b", "é", "a\n", &"a".repeat(65)] {
        assert!(validate_group_id(id).is_err(), "{id:?}");
    }
    for id in ["a", "A-0_", &"a".repeat(64)] {
        assert!(validate_group_id(id).is_ok());
    }
    assert!(parse_group_ids(&raw(&["a", "bad/id"])).is_err());
}
#[test]
fn affinity_values_json() {
    let g = Group {
        id: "g".into(),
        ..Default::default()
    };
    assert_eq!(g.to_string(), r#"{"id":"g","create_timestamp":0}"#);
    let mut runtime = RuntimeGroupInfo::default();
    runtime.group = g;
    runtime.group.leader_store_id = 1;
    runtime.group.voter_store_ids = vec![1, 2];
    let s = runtime.state(100);
    let v = serde_json::to_value(&s).unwrap();
    assert!(v.get("regular_scheduling_allowed").is_none());
    assert_eq!(v["phase"], "pending");
    assert_eq!(
        serde_json::from_value::<GroupState>(v).unwrap().group,
        s.group
    );
    // Go encoding/json treats null scalar/slice fields as their zero value.
    let g: Group =
        serde_json::from_str(r#"{"id":null,"create_timestamp":null,"voter_store_ids":null}"#)
            .unwrap();
    assert_eq!(g, Group::default());
}
#[test]
fn affinity_values_availability_phase() {
    let mut r = RuntimeGroupInfo::default();
    assert!(r.state(100).regular_scheduling_allowed);
    r.group.leader_store_id = 1;
    r.group.voter_store_ids = vec![1, 2];
    r.range_count = 1;
    assert_eq!(r.state(100).phase, "preparing");
    r.region_count = 2;
    r.affinity_region_count = 2;
    assert_eq!(r.state(100).phase, "stable");
    r.set_availability(Availability::Degraded, 100);
    assert_eq!(r.availability(700), Availability::Degraded);
    assert!(!r.state(700).regular_scheduling_allowed);
    assert!(!r.state(700).affinity_scheduling_allowed);
    r.set_availability(Availability::Degraded, 500);
    assert_eq!(r.availability(701), Availability::Expired);
    r.set_availability(Availability::Degraded, 701);
    assert_eq!(r.availability(701), Availability::Expired);
    assert!(r.state(701).regular_scheduling_allowed);
    assert_eq!(r.state(701).phase, "pending");
    r.set_availability(Availability::Available, 702);
    assert_eq!(r.state(702).phase, "stable");
    r.range_count = 0;
    assert_eq!(r.state(702).phase, "pending");
}
struct Provider;
impl GroupStateProvider for Provider {
    fn all_group_states(&self) -> Vec<GroupState> {
        vec![
            self.group_state("a").unwrap(),
            self.group_state("b").unwrap(),
        ]
    }
    fn group_state(&self, id: &str) -> Option<GroupState> {
        ["a", "b"].contains(&id).then(|| GroupState {
            group: Group {
                id: id.into(),
                ..Default::default()
            },
            ..Default::default()
        })
    }
}
#[test]
fn affinity_values_collection() {
    assert_eq!(collect_group_states(&Provider, &[]).unwrap().len(), 2);
    assert!(
        collect_group_states(&Provider, &raw(&[" "]))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        collect_group_states(&Provider, &raw(&["a", "a", "absent"]))
            .unwrap()
            .len(),
        1
    );
    assert!(collect_group_states(&Provider, &raw(&["bad/id"])).is_err());
}
#[test]
fn affinity_values_go_runtime_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule/affinity/task31/go.json"
    ))
    .unwrap();
    struct Actual(Vec<GroupState>);
    impl GroupStateProvider for Actual {
        fn all_group_states(&self) -> Vec<GroupState> {
            self.0.clone()
        }
        fn group_state(&self, id: &str) -> Option<GroupState> {
            self.0.iter().find(|s| s.group.id == id).cloned()
        }
    }
    let provider = Actual(
        fixture["created"]["affinity_groups"]
            .as_object()
            .unwrap()
            .values()
            .map(|v| serde_json::from_value(v.clone()).unwrap())
            .collect(),
    );
    for case in fixture["cases"].as_array().unwrap() {
        let raw: Vec<String> = serde_json::from_value(case["raw"].clone()).unwrap();
        match collect_group_states(&provider, &raw) {
            Ok(states) => {
                assert_eq!(case["status"], 200);
                assert_eq!(
                    serde_json::to_value(states).unwrap(),
                    case["response"]["affinity_groups"]
                );
            }
            Err(err) => {
                assert_eq!(case["status"], 400);
                assert_eq!(err.to_string(), case["response"].as_str().unwrap());
            }
        }
    }
}

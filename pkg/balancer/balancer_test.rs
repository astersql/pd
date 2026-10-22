// Copyright 2026 AsterSQL.

use super::{Policy, RoundRobin, gen_by_policy};

#[test]
fn policy_and_membership() {
    for (policy, label) in [
        (Policy(0), "round-robin"),
        (Policy(1), "least"),
        (Policy(-1), "unknown"),
        (Policy(99), "unknown"),
    ] {
        assert_eq!(policy.to_string(), label);
        let balancer = gen_by_policy::<u32>(policy);
        assert_eq!(balancer.len(), 0);
        assert!(balancer.get_all().is_empty());
        for node in [7, 0, 42, 7] {
            balancer.put(node);
        }
        assert_eq!(balancer.len(), 3);
        assert_eq!(balancer.get_all(), [7, 0, 42]);
        let mut snapshot = balancer.get_all();
        snapshot[0] = 100;
        balancer.put(8);
        assert_eq!(balancer.get_all(), [7, 0, 42, 8]);
    }
    assert_eq!(Policy::ROUND_ROBIN, Policy(0));
    assert_eq!(Policy::LEAST, Policy(1));
    let strings = RoundRobin::<String>::new();
    for node in ["z", "", "a", "z"] {
        strings.put(node.to_owned());
    }
    assert_eq!(strings.get_all(), ["z", "", "a"]);
    assert_eq!(strings.len(), 3);
    let empty = RoundRobin::<u32>::default();
    assert!(empty.is_empty());
}

#[test]
fn concurrent_membership() {
    let balancer = RoundRobin::<u32>::new();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let balancer = &balancer;
            scope.spawn(move || {
                for node in 0..100 {
                    balancer.put(node);
                    let snapshot = balancer.get_all();
                    assert!(snapshot.contains(&node));
                    assert!(balancer.len() <= 100);
                }
            });
        }
    });
    assert_eq!(balancer.len(), 100);
    assert_eq!(balancer.get_all(), (0..100).collect::<Vec<_>>());
}

// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_schedule_operator::*;
use serde_json::Value;
fn step(s: &str) -> OpStep {
    match s {
        "l" | "t" | "u" | "z" => TransferLeader {
            from_store: if s == "z" { 9 } else { 1 },
            to_store: if s == "z" { 1 } else { 2 },
            to_stores: if s == "t" {
                vec![2, 3]
            } else if s == "u" {
                vec![3, 2]
            } else {
                vec![]
            },
        }
        .into(),
        "a" | "A" | "b" => AddLearner {
            to_store: if s == "b" { 3 } else { 2 },
            peer_id: if s == "A" { 999 } else { 1 },
            ..Default::default()
        }
        .into(),
        "p" | "P" => PromoteLearner {
            to_store: if s == "P" { 99 } else { 2 },
            ..Default::default()
        }
        .into(),
        "r" | "R" | "R2" => RemovePeer {
            from_store: if s == "R" { 9 } else { 1 },
            peer_id: if s == "R2" { 99 } else { 1 },
            ..Default::default()
        }
        .into(),
        "e" => ChangePeerV2Enter::default().into(),
        "v" => ChangePeerV2Leave::default().into(),
        "m" | "M" => MergeRegion {
            is_passive: s == "M",
            ..Default::default()
        }
        .into(),
        _ => AddPeer::default().into(),
    }
}
fn steps(v: &Value) -> Vec<OpStep> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| step(s.as_str().unwrap()))
        .collect()
}
#[test]
fn task_133_behavior_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_133.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let mut accepted = 0;
    let mut rejected = 0;
    for (i, c) in cases.iter().enumerate() {
        let kind = OpKind(c["required"].as_u64().unwrap_or(0) as u32);
        let source = 1;
        let target = c["target"].as_u64().unwrap_or(2);
        let expected = match c["check"].as_str().unwrap() {
            "leader" => OperatorExpectation::TransferLeader {
                kind,
                source,
                target,
            },
            "from" => OperatorExpectation::TransferLeaderFrom { kind, source },
            "sources" => OperatorExpectation::MultiSourceLeader {
                kind,
                sources: vec![1, 4],
            },
            "targets" => OperatorExpectation::MultiTargetLeader {
                kind,
                source,
                targets: vec![2, 3],
            },
            "peer" => OperatorExpectation::TransferPeer {
                kind,
                source,
                target,
            },
            "learner" => OperatorExpectation::TransferLearner {
                kind,
                source,
                target,
            },
            "peer_leader" => OperatorExpectation::TransferPeerWithLeader {
                kind,
                source,
                target,
            },
            "add" => OperatorExpectation::AddPeer { kind, store: 2 },
            "remove" => OperatorExpectation::RemovePeer { store: 1 },
            "merge" => {
                OperatorExpectation::MergeSteps(if c.get("expected").is_some_and(Value::is_null) {
                    None
                } else {
                    Some(steps(c.get("expected").unwrap_or(&c["steps"])))
                })
            }
            _ => unreachable!(),
        };
        let op = Operator::new(
            "oracle",
            "oracle",
            1,
            None,
            OpKind(c["kind"].as_u64().unwrap() as u32),
            1,
            steps(&c["steps"]),
        );
        let before = (op.kind(), op.len(), op.current_step(), op.status());
        let result = check_operator(
            if c["missing"].as_bool() == Some(true) {
                None
            } else {
                Some(&op)
            },
            &expected,
        );
        assert_eq!(
            result.is_ok(),
            c["ok"].as_bool().unwrap(),
            "case {i}: {c} result={result:?}"
        );
        assert_eq!(
            before,
            (op.kind(), op.len(), op.current_step(), op.status())
        );
        if result.is_ok() {
            accepted += 1
        } else {
            rejected += 1;
            assert!(!result.unwrap_err().to_string().is_empty());
        }
    }
    println!(
        "{} frozen cases: {accepted} accepted, {rejected} rejected",
        cases.len()
    );
}
#[test]
fn task_133_structured_errors() {
    let expectation = OperatorExpectation::AddPeer {
        kind: OpKind::REPLICA,
        store: 2,
    };
    assert_eq!(
        check_operator(None, &expectation),
        Err(OperatorMismatch::MissingOperator)
    );
    let op = Operator::new("", "", 1, None, OpKind::default(), 0, vec![]);
    assert_eq!(
        check_operator(Some(&op), &expectation),
        Err(OperatorMismatch::StepCount {
            expected: vec![2],
            actual: 0
        })
    );
    let op = Operator::new(
        "",
        "",
        1,
        None,
        OpKind::default(),
        0,
        vec![step("a"), step("p")],
    );
    assert_eq!(
        check_operator(Some(&op), &expectation),
        Err(OperatorMismatch::Kind {
            required: OpKind::REPLICA | OpKind::REGION,
            actual: OpKind::default()
        })
    );
}

// Copyright 2026 AsterSQL.
use astersql_pd_schedule_operator::{OpKind, OpStep, Operator, TransferLeader as LeaderStep};

/// Semantic expectation corresponding to the Go operatorutil checks.
#[derive(Clone, Debug)]
pub enum OperatorExpectation {
    TransferLeader {
        kind: OpKind,
        source: u64,
        target: u64,
    },
    TransferLeaderFrom {
        kind: OpKind,
        source: u64,
    },
    MultiSourceLeader {
        kind: OpKind,
        sources: Vec<u64>,
    },
    MultiTargetLeader {
        kind: OpKind,
        source: u64,
        targets: Vec<u64>,
    },
    TransferPeer {
        kind: OpKind,
        source: u64,
        target: u64,
    },
    TransferLearner {
        kind: OpKind,
        source: u64,
        target: u64,
    },
    TransferPeerWithLeader {
        kind: OpKind,
        source: u64,
        target: u64,
    },
    AddPeer {
        kind: OpKind,
        store: u64,
    },
    RemovePeer {
        store: u64,
    },
    MergeSteps(Option<Vec<OpStep>>),
}

/// A comparison failure, without panics or a dependency on an assertion framework.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperatorMismatch {
    MissingOperator,
    MissingExpectedSteps,
    StepCount { expected: Vec<usize>, actual: usize },
    Step { index: usize },
    Kind { required: OpKind, actual: OpKind },
    LeaderDestination { source: u64, last: u64 },
}
impl std::fmt::Display for OperatorMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OperatorMismatch {}
fn count(steps: &[&OpStep], allowed: &[usize]) -> Result<(), OperatorMismatch> {
    if allowed.contains(&steps.len()) {
        Ok(())
    } else {
        Err(OperatorMismatch::StepCount {
            expected: allowed.to_vec(),
            actual: steps.len(),
        })
    }
}
fn require_step(ok: bool, index: usize) -> Result<(), OperatorMismatch> {
    if ok {
        Ok(())
    } else {
        Err(OperatorMismatch::Step { index })
    }
}
fn kind(op: &Operator, required: OpKind) -> Result<(), OperatorMismatch> {
    if op.kind().0 & required.0 == required.0 {
        Ok(())
    } else {
        Err(OperatorMismatch::Kind {
            required,
            actual: op.kind(),
        })
    }
}

/// Compare a real operator using Go's field projections and kind containment rules.
/// Wrong step variants return an error where Go type assertions would panic.
pub fn check_operator(
    op: Option<&Operator>,
    expected: &OperatorExpectation,
) -> Result<(), OperatorMismatch> {
    use OperatorExpectation::*;
    let op = op.ok_or(OperatorMismatch::MissingOperator)?;
    let all: Vec<_> = (0..op.len())
        .map(|i| op.step(i).expect("operator index in bounds"))
        .collect();
    match expected {
        TransferLeader {
            kind: k, source, ..
        }
        | TransferLeaderFrom { kind: k, source }
        | MultiTargetLeader {
            kind: k, source, ..
        } => {
            count(&all, &[1])?;
            let ok = match (all[0], expected) {
                (OpStep::TransferLeader(s), TransferLeader { target, .. }) => {
                    *s == LeaderStep {
                        from_store: *source,
                        to_store: *target,
                        to_stores: vec![],
                    }
                }
                (OpStep::TransferLeader(s), TransferLeaderFrom { .. }) => s.from_store == *source,
                (OpStep::TransferLeader(s), MultiTargetLeader { targets, .. }) => {
                    s.from_store == *source
                        && targets.contains(&s.to_store)
                        && s.to_stores == *targets
                }
                _ => false,
            };
            require_step(ok, 0)?;
            kind(op, *k | OpKind::LEADER)
        }
        MultiSourceLeader { kind: k, sources } => {
            count(&all, &[1])?;
            require_step(
                matches!(all[0], OpStep::TransferLeader(s) if sources.contains(&s.from_store)),
                0,
            )?;
            kind(op, *k | OpKind::LEADER)
        }
        TransferPeer {
            kind: k,
            source,
            target,
        }
        | TransferPeerWithLeader {
            kind: k,
            source,
            target,
        }
        | TransferLearner {
            kind: k,
            source,
            target,
        } => {
            let steps: Vec<_> = all
                .iter()
                .copied()
                .filter(|s| !matches!(s, OpStep::TransferLeader(_)))
                .collect();
            if matches!(expected, TransferLearner { .. }) {
                count(&steps, &[2])?;
            } else {
                count(&steps, &[3, 4])?;
                if steps.len() == 3 {
                    require_step(matches!(steps[1], OpStep::PromoteLearner(_)), 1)?;
                } else {
                    require_step(matches!(steps[1], OpStep::ChangePeerV2Enter(_)), 1)?;
                    require_step(matches!(steps[2], OpStep::ChangePeerV2Leave(_)), 2)?;
                }
            }
            let wildcard = matches!(expected, TransferPeerWithLeader { target: 0, .. });
            require_step(
                matches!(steps[0], OpStep::AddLearner(s) if wildcard || s.to_store == *target),
                0,
            )?;
            let last = steps.len() - 1;
            require_step(
                matches!(steps[last], OpStep::RemovePeer(s) if s.from_store == *source),
                last,
            )?;
            if matches!(expected, TransferPeerWithLeader { .. }) {
                let last = all
                    .iter()
                    .filter_map(|s| {
                        if let OpStep::TransferLeader(s) = s {
                            Some(s.to_store)
                        } else {
                            None
                        }
                    })
                    .next_back()
                    .unwrap_or(0);
                if last == 0 || last == *source {
                    return Err(OperatorMismatch::LeaderDestination {
                        source: *source,
                        last,
                    });
                }
            }
            kind(op, *k | OpKind::REGION)
        }
        AddPeer { kind: k, store } => {
            count(&all, &[2])?;
            require_step(
                matches!(all[0],OpStep::AddLearner(s) if s.to_store==*store),
                0,
            )?;
            require_step(matches!(all[1], OpStep::PromoteLearner(_)), 1)?;
            kind(op, *k | OpKind::REGION)
        }
        RemovePeer { store } => {
            count(&all, &[1, 2])?;
            if all.len() == 2 {
                require_step(
                    matches!(all[0],OpStep::TransferLeader(s) if s.from_store==*store),
                    0,
                )?;
            }
            require_step(
                matches!(all[all.len()-1],OpStep::RemovePeer(s) if s.from_store==*store),
                all.len() - 1,
            )
        }
        MergeSteps(steps) => {
            kind(op, OpKind::MERGE)?;
            let steps = steps
                .as_ref()
                .ok_or(OperatorMismatch::MissingExpectedSteps)?;
            count(&all, &[steps.len()])?;
            for (i, (a, b)) in all.iter().zip(steps).enumerate() {
                let ok = match (a, b) {
                    (OpStep::AddLearner(a), OpStep::AddLearner(b)) => a.to_store == b.to_store,
                    (OpStep::PromoteLearner(a), OpStep::PromoteLearner(b)) => {
                        a.to_store == b.to_store
                    }
                    (OpStep::TransferLeader(a), OpStep::TransferLeader(b)) => {
                        a.from_store == b.from_store && a.to_store == b.to_store
                    }
                    (OpStep::RemovePeer(a), OpStep::RemovePeer(b)) => a.from_store == b.from_store,
                    (OpStep::MergeRegion(a), OpStep::MergeRegion(b)) => {
                        a.is_passive == b.is_passive
                    }
                    _ => false,
                };
                require_step(ok, i)?;
            }
            Ok(())
        }
    }
}

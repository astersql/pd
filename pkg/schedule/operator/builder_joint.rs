// Copyright 2026 AsterSQL.
//! Joint membership planning reuses the ordinary planner's canonical leader preferences.
use super::*;
use crate::{ChangePeerV2Enter, ChangePeerV2Leave, DemoteVoter};

impl<V> Builder<'_, V> {
    /// Build with joint consensus enabled. Single non-demotion changes use the
    /// ordinary path, except a removal leaving one peer. Witness conversions
    /// are batched after membership changes and before data-voter restoration.
    pub async fn build_joint<F>(
        &mut self,
        kind: OpKind,
        options: SimpleBuildOptions,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let result = self.build_joint_inner(kind, options, allocate).await;
        if let Err(error) = &result {
            self.reject(error.clone());
        }
        result
    }
    async fn build_joint_inner<F>(
        &mut self,
        kind: OpKind,
        options: SimpleBuildOptions,
        mut allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        self.validate_targets().await?;
        let mut p = Planner {
            builder: self,
            current: self.origin_peers.clone(),
            add: BTreeMap::new(),
            remove: BTreeMap::new(),
            promote: BTreeMap::new(),
            added: BTreeMap::new(),
            leader: self.origin_leader,
            steps: vec![],
            kind,
            options,
        };
        let mut demote = BTreeMap::new();
        let mut witness = crate::builder_witness::WitnessPlan::default();
        for (&s, o) in &self.origin_peers {
            match self.target_peers.get(&s) {
                None => {
                    p.remove.insert(s, o.clone());
                }
                Some(n) => {
                    let n = witness.peer(o, n);
                    if o.is_learner() && !n.is_learner() {
                        p.promote.insert(s, n);
                    } else if !o.is_learner() && n.is_learner() {
                        demote.insert(s, n);
                    }
                }
            }
        }
        if witness.restore.contains_key(&self.target_leader) {
            return Err(BuilderError::LeaderNotAllowed);
        }
        for (&s, n) in &self.target_peers {
            if !self.origin_peers.contains_key(&s) {
                p.add.insert(s, n.clone());
            }
        }
        if p.add.len() + p.remove.len() + p.promote.len() + witness.len() <= 1
            && demote.is_empty()
            && !(p.remove.len() == 1 && self.target_peers.len() == 1)
        {
            return self.build_simple(kind, options, allocate).await;
        }
        for n in p.add.values_mut() {
            if n.id == 0 {
                n.id = allocate()?;
            }
        }
        // Add all peers as learners; promotion is batched into the V2 command.
        for (&s, n) in &p.add {
            p.steps.push(OpStep::AddLearner(AddLearner {
                to_store: s,
                peer_id: n.id,
                send_store: self.origin_leader,
                is_light_weight: options.add_light_peer,
                is_witness: n.is_witness,
            }));
            p.added.insert(s, p.steps.len());
            p.current.insert(s, n.clone());
            if !n.is_learner() {
                p.promote.insert(s, n.clone());
            }
            p.kind.0 |= OpKind::REGION.0;
        }
        let target = if witness.restore.is_empty() {
            p.select_target_leader().await
        } else {
            p.select_target_leader_excluding(&witness.restore).await
        };
        if target == 0 {
            return Err(BuilderError::NoValidLeader);
        }
        for (&s, n) in &p.remove {
            if !n.is_learner() {
                demote.insert(s, n.clone());
            }
        }
        let enter = ChangePeerV2Enter {
            promote_learners: p
                .promote
                .values()
                .map(|n| PromoteLearner {
                    to_store: n.store_id,
                    peer_id: n.id,
                    is_witness: n.is_witness,
                })
                .collect(),
            demote_voters: demote
                .values()
                .map(|n| DemoteVoter {
                    to_store: n.store_id,
                    peer_id: n.id,
                    is_witness: n.is_witness,
                })
                .collect(),
        };
        let before = self
            .origin_peers
            .get(&target)
            .is_some_and(|n| !n.is_learner());
        let during = !before
            && !self
                .target_peers
                .get(&self.origin_leader)
                .is_some_and(|n| !n.is_learner());
        if before {
            p.transfer(target);
        }
        let changes = enter.promote_learners.len() + enter.demote_voters.len();
        if changes != 0 {
            p.steps.push(OpStep::ChangePeerV2Enter(enter.clone()));
        }
        if during {
            p.transfer(target);
        }
        // TiKV stabilizes a single membership change automatically.
        if changes > 1 {
            p.steps.push(OpStep::ChangePeerV2Leave(ChangePeerV2Leave {
                promote_learners: enter.promote_learners,
                demote_voters: enter.demote_voters,
            }));
        }
        if !before && !during {
            p.transfer(target);
        }
        for n in p.remove.values() {
            let down = self.context.stores.get_store(n.store_id).is_some_and(|s| {
                i128::from(s.down_time_nanos(self.context.now_unix_nanos))
                    > self.context.config.get_max_store_down_time().as_nanos() as i128
            });
            p.steps.push(OpStep::RemovePeer(RemovePeer {
                from_store: n.store_id,
                peer_id: n.id,
                is_light_weight: options.remove_light_peer,
                is_down_store: down,
            }));
            p.kind.0 |= OpKind::REGION.0;
        }
        if !witness.witnesses.is_empty() || !witness.non_witnesses.is_empty() {
            p.steps
                .push(OpStep::BatchSwitchWitness(crate::BatchSwitchWitness {
                    to_witnesses: witness
                        .witnesses
                        .values()
                        .map(|n| crate::BecomeWitness {
                            store_id: n.store_id,
                            peer_id: n.id,
                        })
                        .collect(),
                    to_non_witnesses: witness
                        .non_witnesses
                        .values()
                        .map(|n| crate::BecomeNonWitness {
                            store_id: n.store_id,
                            peer_id: n.id,
                            send_store: self.origin_leader,
                        })
                        .collect(),
                }));
        }
        if !witness.restore.is_empty() {
            let promotions: Vec<_> = witness
                .restore
                .values()
                .map(|n| PromoteLearner {
                    to_store: n.store_id,
                    peer_id: n.id,
                    is_witness: false,
                })
                .collect();
            p.steps.push(OpStep::ChangePeerV2Enter(ChangePeerV2Enter {
                promote_learners: promotions.clone(),
                demote_voters: vec![],
            }));
            if promotions.len() > 1 {
                p.steps.push(OpStep::ChangePeerV2Leave(ChangePeerV2Leave {
                    promote_learners: promotions,
                    demote_voters: vec![],
                }));
            }
            p.kind.0 |= OpKind::REGION.0;
        }
        if p.steps.is_empty() {
            return Err(BuilderError::NoSteps);
        }
        let op = Operator::new(
            &self.description,
            "joint replica changes",
            self.region.meta().id,
            self.region.meta().epoch.clone(),
            p.kind,
            self.region.stats().approximate_size,
            p.steps,
        );
        self.target_leader = target;
        Ok(op)
    }
}

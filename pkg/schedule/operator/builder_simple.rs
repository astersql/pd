// Copyright 2026 AsterSQL.
//! Ordinary (non-joint) replica planning over canonical builder snapshots.
use crate::{
    AddLearner, Builder, BuilderError, OpKind, OpStep, Operator, PromoteLearner, RemovePeer,
    TransferLeader,
};
use astersql_pd_core::Peer;
use std::collections::BTreeMap;

/// Snapshot accounting options; they do not bypass safety or leader admission.
#[derive(Clone, Copy, Debug, Default)]
pub struct SimpleBuildOptions {
    pub add_light_peer: bool,
    pub remove_light_peer: bool,
}
#[derive(Clone, Default)]
struct Plan {
    before_add: u64,
    before_remove: u64,
    add: Option<Peer>,
    remove: Option<Peer>,
    promote: Option<Peer>,
}
struct Planner<'b, 'a, V> {
    builder: &'b Builder<'a, V>,
    current: BTreeMap<u64, Peer>,
    add: BTreeMap<u64, Peer>,
    remove: BTreeMap<u64, Peer>,
    promote: BTreeMap<u64, Peer>,
    added: BTreeMap<u64, usize>,
    leader: u64,
    steps: Vec<OpStep>,
    kind: OpKind,
    options: SimpleBuildOptions,
}
impl<'a, V> Builder<'a, V> {
    /// Plan ordinary Add/Promote/Remove and leader transfers. The host supplies
    /// IDs only for new zero-ID peers; allocator errors propagate without an operator.
    /// Joint and witness conversion planning remain separate behavior families.
    pub async fn build_simple<F>(
        &mut self,
        kind: OpKind,
        options: SimpleBuildOptions,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let result = self.build_simple_inner(kind, options, allocate).await;
        if let Err(error) = &result {
            self.reject(error.clone());
        }
        result
    }
    async fn build_simple_inner<F>(
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
        for (&s, o) in &self.origin_peers {
            match self.target_peers.get(&s) {
                None => {
                    p.remove.insert(s, o.clone());
                }
                Some(n) => {
                    if o.is_witness != n.is_witness {
                        return Err(BuilderError::PlanningRequired);
                    }
                    if o.is_learner() && !n.is_learner() {
                        let mut n = n.clone();
                        n.id = o.id;
                        p.promote.insert(s, n);
                    } else if !o.is_learner() && n.is_learner() {
                        p.remove.insert(s, o.clone());
                    }
                }
            }
        }
        for (&s, n) in &self.target_peers {
            if !self.origin_peers.contains_key(&s)
                || self
                    .origin_peers
                    .get(&s)
                    .is_some_and(|o| !o.is_learner() && n.is_learner())
            {
                let mut n = n.clone();
                if n.id == 0 {
                    n.id = allocate()?;
                }
                p.add.insert(s, n);
            }
        }
        while !p.add.is_empty() || !p.remove.is_empty() || !p.promote.is_empty() {
            let plan = p.next().await.ok_or(BuilderError::NoValidLeader)?;
            p.transfer(plan.before_add);
            if let Some(n) = plan.add {
                p.steps.push(OpStep::AddLearner(AddLearner {
                    to_store: n.store_id,
                    peer_id: n.id,
                    send_store: self.origin_leader,
                    is_light_weight: options.add_light_peer,
                    is_witness: n.is_witness,
                }));
                if !n.is_learner() {
                    p.promote_step(&n);
                }
                p.current.insert(n.store_id, n.clone());
                p.add.remove(&n.store_id);
                p.added.insert(n.store_id, p.steps.len());
                p.kind.0 |= OpKind::REGION.0;
            }
            if let Some(n) = plan.promote {
                p.promote_step(&n);
                p.current.insert(n.store_id, n.clone());
                p.promote.remove(&n.store_id);
            }
            p.transfer(plan.before_remove);
            if let Some(n) = plan.remove {
                let down = self.context.stores.get_store(n.store_id).is_some_and(|s| {
                    i128::from(s.down_time_nanos(self.context.now_unix_nanos))
                        > self.context.config.get_max_store_down_time().as_nanos() as i128
                });
                p.steps.push(OpStep::RemovePeer(RemovePeer {
                    from_store: n.store_id,
                    peer_id: n.id,
                    is_light_weight: p.options.remove_light_peer,
                    is_down_store: down,
                }));
                p.current.remove(&n.store_id);
                p.remove.remove(&n.store_id);
                p.kind.0 |= OpKind::REGION.0;
            }
        }
        let target = p.select_target_leader().await;
        if p.current.contains_key(&target) {
            p.transfer(target);
        }
        if p.steps.is_empty() {
            return Err(BuilderError::NoSteps);
        }
        let op = Operator::new(
            &self.description,
            "ordinary replica changes",
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
impl<V> Planner<'_, '_, V> {
    fn transfer(&mut self, to: u64) {
        if to != 0 && to != self.leader {
            self.steps.push(OpStep::TransferLeader(TransferLeader {
                from_store: self.leader,
                to_store: to,
                to_stores: self.builder.target_leaders.clone(),
            }));
            self.leader = to;
            self.kind.0 |= OpKind::LEADER.0;
        }
    }
    fn promote_step(&mut self, n: &Peer) {
        self.steps.push(OpStep::PromoteLearner(PromoteLearner {
            to_store: n.store_id,
            peer_id: n.id,
            is_witness: n.is_witness,
        }));
    }
    async fn next(&self) -> Option<Plan> {
        let mut best = None;
        for a in self.add.values() {
            for r in self.remove.values() {
                if a.is_learner() == r.is_learner() {
                    self.replace(
                        &mut best,
                        Plan {
                            add: Some(a.clone()),
                            remove: Some(r.clone()),
                            ..Default::default()
                        },
                    )
                    .await;
                }
            }
        }
        for n in self.promote.values() {
            for a in self.add.values().filter(|a| a.is_learner()) {
                for r in self
                    .remove
                    .values()
                    .filter(|r| !r.is_learner() && r.store_id != a.store_id)
                {
                    self.replace(
                        &mut best,
                        Plan {
                            add: Some(a.clone()),
                            promote: Some(n.clone()),
                            remove: Some(r.clone()),
                            ..Default::default()
                        },
                    )
                    .await;
                }
            }
        }
        if best.is_some() {
            return best;
        }
        if let Some(n) = self.promote.values().next() {
            return Some(Plan {
                promote: Some(n.clone()),
                ..Default::default()
            });
        }
        for r in self.remove.values() {
            for n in self.current.values() {
                if n.store_id != r.store_id
                    && self.builder.allow_leader_at(n, false, self.leader).await
                {
                    self.best(
                        &mut best,
                        Plan {
                            remove: Some(r.clone()),
                            before_remove: n.store_id,
                            ..Default::default()
                        },
                    );
                }
            }
        }
        if best.is_some() {
            return best;
        }
        for a in self.add.values() {
            for n in self.current.values() {
                if self.builder.allow_leader_at(n, false, self.leader).await {
                    self.best(
                        &mut best,
                        Plan {
                            add: Some(a.clone()),
                            before_add: n.store_id,
                            ..Default::default()
                        },
                    );
                }
            }
        }
        best
    }
}

#[path = "builder_leader.rs"]
mod leader;

#[path = "builder_joint.rs"]
mod joint;

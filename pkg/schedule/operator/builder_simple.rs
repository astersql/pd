// Copyright 2026 AsterSQL.
//! Ordinary (non-joint) replica planning over canonical builder snapshots.
use crate::{
    AddLearner, Builder, BuilderError, OpKind, OpStep, Operator, PromoteLearner, RemovePeer,
    TransferLeader,
};
use astersql_pd_core::{Peer, equal_fold};
use astersql_pd_core_constant::SchedulePolicy;
use astersql_pd_schedule_placement::PeerRoleType;
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
        let mut target = self.target_leader;
        if target == 0 {
            for (&s, n) in &self.target_peers {
                if self.expected_roles.get(&s) == Some(&PeerRoleType::Follower)
                    || !self.allow_leader(n, self.force_target_leader).await
                {
                    continue;
                }
                if target == 0
                    || p.leader_rank(s) > p.leader_rank(target)
                    || (p.leader_rank(s) == p.leader_rank(target) && p.lower_load(s, target))
                {
                    target = s;
                }
            }
        }
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
    fn up(&self, s: u64) -> i64 {
        i64::from(
            self.builder
                .context
                .stores
                .get_store(s)
                .is_some_and(|s| s.is_preparing() || s.is_serving()),
        )
    }
    fn age(&self, s: u64) -> i64 {
        *self.added.get(&s).unwrap_or(&0) as i64
    }
    fn label_match(&self, x: u64, y: u64) -> i64 {
        let stores = self.builder.context.stores;
        let (Some(x), Some(y)) = (stores.get_store(x), stores.get_store(y)) else {
            return 0;
        };
        self.builder
            .context
            .config
            .get_location_labels()
            .iter()
            .take_while(|l| equal_fold(x.label_value(l), y.label_value(l)))
            .count() as i64
    }
    fn rank(&self, p: &Plan) -> [i64; 6] {
        let target = self.builder.target_leader;
        let mut nearest = match (&p.add, &p.remove) {
            (Some(a), Some(r)) => self.label_match(a.store_id, r.store_id),
            _ => 0,
        };
        if let (Some(a), Some(n)) = (&p.add, &p.promote) {
            nearest = nearest.min(self.label_match(n.store_id, a.store_id));
        }
        let old = -self.age(p.before_add)
            - if p
                .add
                .as_ref()
                .is_some_and(|a| a.store_id == p.before_remove)
            {
                self.steps.len() as i64 + 1
            } else {
                self.age(p.before_remove)
            };
        let add_target = i64::from(
            target != 0
                && (p
                    .add
                    .as_ref()
                    .is_some_and(|a| a.store_id == target && !a.is_learner())
                    || p.promote.as_ref().is_some_and(|a| a.store_id == target)),
        );
        let target_leader = i64::from(
            target == 0
                || (p.before_remove != 0 && p.before_remove == target)
                || (p.before_remove == 0 && p.before_add == target),
        );
        let less = if p.before_add == 0 || p.before_add == self.leader {
            2 + i64::from(p.before_remove == 0 || p.before_remove == self.leader)
        } else {
            i64::from(p.before_remove == 0 || p.before_remove == p.before_add)
        };
        [
            nearest,
            if p.add.is_some() {
                self.up(p.before_add)
            } else {
                1
            },
            old,
            add_target,
            target_leader,
            less,
        ]
    }
    fn best(&self, best: &mut Option<Plan>, next: Plan) {
        if best
            .as_ref()
            .is_none_or(|b| self.rank(&next) > self.rank(b))
        {
            *best = Some(next);
        }
    }
    async fn replace(&self, best: &mut Option<Plan>, plan: Plan) {
        for (&before_add, n) in &self.current {
            if !self.builder.allow_leader(n, false).await {
                continue;
            }
            for n in self
                .current
                .values()
                .chain(plan.promote.iter())
                .chain(plan.add.iter())
            {
                if plan
                    .remove
                    .as_ref()
                    .is_some_and(|r| r.store_id == n.store_id)
                    || !self.builder.allow_leader(n, false).await
                {
                    continue;
                }
                self.best(
                    best,
                    Plan {
                        before_add,
                        before_remove: n.store_id,
                        ..plan.clone()
                    },
                );
            }
        }
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
                if n.store_id != r.store_id && self.builder.allow_leader(n, false).await {
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
                if self.builder.allow_leader(n, false).await {
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
    fn leader_rank(&self, s: u64) -> [i64; 5] {
        [
            i64::from(self.builder.expected_roles.get(&s) == Some(&PeerRoleType::Leader)),
            self.up(s),
            i64::from(s == self.leader),
            i64::from(!self.promote.contains_key(&s)),
            -self.age(s),
        ]
    }
    fn lower_load(&self, a: u64, b: u64) -> bool {
        let (Some(a), Some(b)) = (
            self.builder.context.stores.get_store(a),
            self.builder.context.stores.get_store(b),
        ) else {
            return false;
        };
        let ac = a.leader_score(SchedulePolicy::BY_COUNT, 0);
        let bc = b.leader_score(SchedulePolicy::BY_COUNT, 0);
        ac < bc
            || (ac == bc
                && a.leader_score(SchedulePolicy::BY_SIZE, 0)
                    < b.leader_score(SchedulePolicy::BY_SIZE, 0))
    }
}

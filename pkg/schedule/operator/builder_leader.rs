// Copyright 2026 AsterSQL.
//! Leader and replacement plan preference over the canonical ordinary planner.
use super::*;
use astersql_pd_core::equal_fold;
use astersql_pd_core_constant::SchedulePolicy;
use astersql_pd_schedule_placement::PeerRoleType;

impl<V> Planner<'_, '_, V> {
    pub(super) async fn select_target_leader(&self) -> u64 {
        self.select_target_leader_excluding(&BTreeMap::new()).await
    }
    pub(super) async fn select_target_leader_excluding(
        &self,
        excluded: &BTreeMap<u64, Peer>,
    ) -> u64 {
        let mut target = self.builder.target_leader;
        if target == 0 {
            for (&s, n) in &self.builder.target_peers {
                if excluded.contains_key(&s)
                    || self.builder.expected_roles.get(&s) == Some(&PeerRoleType::Follower)
                    || !self
                        .builder
                        .allow_leader_at(n, self.builder.force_target_leader, self.leader)
                        .await
                {
                    continue;
                }
                if target == 0
                    || self.leader_rank(s) > self.leader_rank(target)
                    || (self.leader_rank(s) == self.leader_rank(target)
                        && self.lower_load(s, target))
                {
                    target = s;
                }
            }
        }
        target
    }

    pub(super) fn up(&self, s: u64) -> i64 {
        i64::from(
            self.builder
                .context
                .stores
                .get_store(s)
                .is_some_and(|s| s.is_preparing() || s.is_serving()),
        )
    }
    pub(super) fn age(&self, s: u64) -> i64 {
        *self.added.get(&s).unwrap_or(&0) as i64
    }
    pub(super) fn label_match(&self, x: u64, y: u64) -> i64 {
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
    pub(super) fn rank(&self, p: &Plan) -> [i64; 6] {
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
    pub(super) fn best(&self, best: &mut Option<Plan>, next: Plan) {
        if best
            .as_ref()
            .is_none_or(|b| self.rank(&next) > self.rank(b))
        {
            *best = Some(next);
        }
    }
    pub(super) async fn replace(&self, best: &mut Option<Plan>, plan: Plan) {
        for (&before_add, n) in &self.current {
            if !self.builder.allow_leader_at(n, false, self.leader).await {
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
                    || !self.builder.allow_leader_at(n, false, self.leader).await
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
    pub(super) fn leader_rank(&self, s: u64) -> [i64; 5] {
        [
            i64::from(self.builder.expected_roles.get(&s) == Some(&PeerRoleType::Leader)),
            self.up(s),
            i64::from(s == self.leader),
            i64::from(!self.promote.contains_key(&s)),
            -self.age(s),
        ]
    }
    pub(super) fn lower_load(&self, a: u64, b: u64) -> bool {
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

#[cfg(test)]
#[path = "task_028_test.rs"]
mod task_028_test;

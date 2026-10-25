// Copyright 2026 AsterSQL.
//! Range, scatter and witness factories over immutable host snapshots.
use crate::*;
use astersql_pd_core::{Peer, RegionInfo, is_in_joint_state};
use astersql_pd_schedule_hbstream::SplitPolicy;
use std::collections::BTreeMap;

fn hex(key: &[u8]) -> String {
    key.iter().map(|b| format!("{b:02X}")).collect()
}
/// Peer IDs, ordering and leader identity do not affect merge membership matching.
pub fn is_region_match(a: &RegionInfo, b: &RegionInfo) -> bool {
    a.meta().peers.len() == b.meta().peers.len()
        && a.meta().peers.iter().all(|p| {
            b.meta()
                .peers
                .iter()
                .find(|n| n.store_id == p.store_id)
                .is_some_and(|n| n.is_learner() == p.is_learner() && n.is_witness == p.is_witness)
        })
}
/// Preserve the caller's policy, binary keys and kind, including unknown policies.
pub fn create_split_region_operator(
    description: &str,
    region: &RegionInfo,
    kind: OpKind,
    policy: SplitPolicy,
    keys: Vec<Vec<u8>>,
) -> Result<Operator, BuilderError> {
    if is_in_joint_state(&region.meta().peers) {
        return Err(BuilderError::OriginJoint);
    }
    let policy_name = match policy {
        SplitPolicy::Scan => "SCAN".into(),
        SplitPolicy::Approximate => "APPROXIMATE".into(),
        SplitPolicy::UseKey => "USEKEY".into(),
        SplitPolicy::Unknown(n) => n.to_string(),
    };
    let mut brief = format!(
        "split: region {} use policy {policy_name}",
        region.meta().id
    );
    if !keys.is_empty() {
        brief.push_str(&format!(
            " and keys [{}]",
            keys.iter().map(|k| hex(k)).collect::<Vec<_>>().join(" ")
        ));
    }
    let op = Operator::new(
        description,
        &brief,
        region.meta().id,
        region.meta().epoch.clone(),
        kind,
        region.stats().approximate_size,
        vec![OpStep::SplitRegion(SplitRegion {
            start_key: region.meta().key_range.start_key.clone(),
            end_key: region.meta().key_range.end_key.clone(),
            policy,
            split_keys: keys,
        })],
    );
    op.additional_info()
        .set("region-start-key", hex(&region.meta().key_range.start_key));
    op.additional_info()
        .set("region-end-key", hex(&region.meta().key_range.end_key));
    Ok(op)
}

/// One host-owned cluster snapshot; randomness is an explicit index into the
/// sorted non-learner candidates, supplied only when no target leader is given.
pub struct RangeOperatorFactory<'a, V> {
    builder: Builder<'a, V>,
    joint: bool,
}
impl<'a, V> RangeOperatorFactory<'a, V> {
    pub fn new(
        description: &str,
        context: BuilderContext<'a, V>,
        region: &RegionInfo,
        supports_conf_change_v2: bool,
    ) -> Self {
        let joint = supports_conf_change_v2 && context.config.is_use_joint_consensus();
        Self {
            builder: Builder::new(description, context, region, BuilderOptions::default()),
            joint,
        }
    }
    async fn finish<F>(
        &mut self,
        kind: OpKind,
        options: SimpleBuildOptions,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        if self.joint {
            self.builder.build_joint(kind, options, allocate).await
        } else {
            self.builder.build_simple(kind, options, allocate).await
        }
    }
    pub async fn create_merge<F>(
        mut self,
        target: &RegionInfo,
        mut kind: OpKind,
        allocate: F,
    ) -> Result<[Operator; 2], BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let source = self.builder.region.clone();
        if is_in_joint_state(&source.meta().peers) || is_in_joint_state(&target.meta().peers) {
            return Err(BuilderError::OriginJoint);
        }
        let mut steps = vec![];
        if !is_region_match(&source, target) {
            let peers = target
                .meta()
                .peers
                .iter()
                .map(|p| (p.store_id, Peer { id: 0, ..p.clone() }))
                .collect();
            self.builder.set_peers(peers);
            let op = self
                .finish(kind, SimpleBuildOptions::default(), allocate)
                .await?;
            steps.extend((0..op.len()).map(|i| op.step(i).unwrap().clone()));
            kind = op.kind();
        }
        let step = MergeRegion {
            from_region: Some(source.meta().clone()),
            to_region: Some(target.meta().clone()),
            is_passive: false,
        };
        steps.push(OpStep::MergeRegion(step.clone()));
        let brief = format!("merge: region {} to {}", source.meta().id, target.meta().id);
        let active = Operator::new(
            &self.builder.description,
            &brief,
            source.meta().id,
            source.meta().epoch.clone(),
            kind,
            source.stats().approximate_size,
            steps,
        );
        let mut passive = Operator::new(
            &self.builder.description,
            &brief,
            target.meta().id,
            target.meta().epoch.clone(),
            kind,
            target.stats().approximate_size,
            vec![OpStep::MergeRegion(MergeRegion {
                is_passive: true,
                ..step
            })],
        );
        passive.sync(&active);
        Ok([active, passive])
    }
    pub async fn create_scatter<F>(
        mut self,
        peers: BTreeMap<u64, Peer>,
        target_leader: u64,
        random_index: usize,
        skip_limit_check: bool,
        admin: bool,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let candidates: Vec<_> = peers
            .iter()
            .filter(|(_, p)| !p.is_learner())
            .map(|(&s, _)| s)
            .collect();
        let leader = if target_leader != 0 {
            target_leader
        } else if candidates.is_empty() {
            0
        } else {
            candidates[random_index % candidates.len()]
        };
        self.builder.set_peers(peers).set_leader(leader);
        if admin {
            self.builder.enable_force_target_leader();
        }
        self.finish(
            if admin {
                OpKind::ADMIN
            } else {
                OpKind::SPLIT_SCATTER
            },
            SimpleBuildOptions {
                add_light_peer: true,
                remove_light_peer: skip_limit_check,
            },
            allocate,
        )
        .await
    }
    pub async fn create_witness<F>(
        mut self,
        store: u64,
        witness: bool,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        if witness {
            self.builder.become_witness(store);
        } else {
            self.builder.become_non_witness(store);
        }
        self.finish(OpKind::WITNESS, SimpleBuildOptions::default(), allocate)
            .await
    }
    pub async fn create_move_witness<F>(
        mut self,
        source: u64,
        target: u64,
        allocate: F,
    ) -> Result<Operator, BuilderError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        self.builder
            .become_non_witness(source)
            .become_witness(target);
        self.finish(OpKind::WITNESS, SimpleBuildOptions::default(), allocate)
            .await
    }
    /// Recovery deliberately skips placement fitting and origin-joint rejection.
    pub async fn create_leave_joint(self) -> Result<Operator, BuilderError> {
        let b = self.builder;
        let mut b = Builder::new(
            &b.description,
            b.context,
            &b.region,
            BuilderOptions {
                skip_origin_joint_state_check: true,
                skip_placement_rules_check: true,
            },
        );
        b.build_leave_joint().await
    }
}

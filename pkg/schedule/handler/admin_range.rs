// Copyright 2026 AsterSQL.
//! Range management through canonical validation, factories and admission.
use crate::{Handler, HandlerError};
use astersql_pd_core::RegionInfo;
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_filter::{
    RegionReplicationContext, is_region_healthy, is_region_replicated,
};
use astersql_pd_schedule_hbstream::SplitPolicy;
use astersql_pd_schedule_operator::{
    AdmissionError, BuilderContext, BuilderError, OpKind, Operator, RangeOperatorFactory,
    create_split_region_operator,
};
use astersql_pd_schedule_placement::RuleManager;
use std::{cell::RefCell, sync::Arc, time::Instant};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AdminRangeError {
    #[error(transparent)]
    Handler(#[from] HandlerError),
    #[error("region {0} not found")]
    RegionNotFound(u64),
    #[error("region {0} has abnormal peer")]
    AbnormalPeer(u64),
    #[error("regions are not adjacent")]
    NotAdjacent,
    #[error("check policy {0} is not supported")]
    UnsupportedPolicy(String),
    #[error("split key {0} is not in hex format")]
    InvalidHexKey(String),
    #[error(transparent)]
    Builder(#[from] BuilderError),
    #[error("failed to add operator: {0}")]
    Admission(AdmissionError),
}
/// Host-owned placement state and explicit time/capabilities for one request.
pub struct AdminRangeContext<'a> {
    pub manager: &'a mut RuleManager,
    pub now_unix_nanos: i128,
    pub supports_conf_change_v2: bool,
    pub now: Instant,
}
impl<C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static> Handler<'_, C> {
    fn range_region(&self, id: u64) -> Result<Arc<RegionInfo>, AdminRangeError> {
        self.coordinator()?
            .get_cluster()
            .regions
            .get_region(id)
            .ok_or(AdminRangeError::RegionNotFound(id))
    }
    fn admit_range(&self, ops: Vec<Arc<Operator>>, now: Instant) -> Result<(), AdminRangeError> {
        let c = self.coordinator()?;
        let outcome = c
            .get_operator_controller()
            .add_operators(c.get_cluster(), &ops)
            .map_err(AdminRangeError::Admission)?;
        self.archive_removed(&outcome.removed, now)?;
        Ok(())
    }
    /// Validate both regions before building and atomically admitting a merge pair.
    pub async fn add_merge_region_operator<F>(
        &self,
        source_id: u64,
        target_id: u64,
        context: AdminRangeContext<'_>,
        allocate: F,
    ) -> Result<(), AdminRangeError>
    where
        F: FnMut() -> Result<u64, BuilderError>,
    {
        let source = self.range_region(source_id)?;
        let target = self.range_region(target_id)?;
        let c = self.coordinator()?;
        let stores = c.get_cluster().stores.get_stores();
        let rules = {
            let replication = RegionReplicationContext {
                conf: &**c.get_config(),
                stores: &stores,
                manager: RefCell::new(context.manager),
                now: context.now_unix_nanos,
            };
            for region in [&source, &target] {
                if !is_region_healthy(region) || !is_region_replicated(&replication, region) {
                    return Err(AdminRangeError::AbnormalPeer(region.meta().id));
                }
            }
            let a = &source.meta().key_range;
            let b = &target.meta().key_range;
            if (a.start_key != b.end_key || a.start_key.is_empty())
                && (a.end_key != b.start_key || a.end_key.is_empty())
            {
                return Err(AdminRangeError::NotAdjacent);
            }
            replication
                .manager
                .borrow()
                .get_rules_for_apply_region(&source)
        };
        let ops = RangeOperatorFactory::new(
            "admin-merge-region",
            BuilderContext {
                stores: &c.get_cluster().stores,
                config: &**c.get_config(),
                rules: &rules,
                now_unix_nanos: context.now_unix_nanos,
            },
            &source,
            context.supports_conf_change_v2,
        )
        .create_merge(&target, OpKind::ADMIN, allocate)
        .await?;
        self.admit_range(ops.into_iter().map(Arc::new).collect(), context.now)
    }
    /// Parse Go policy names case-insensitively; only USEKEY decodes keys.
    /// Range ordering, duplicates and empty keys intentionally pass through.
    pub fn add_split_region_operator(
        &self,
        region_id: u64,
        policy: &str,
        keys: &[String],
        now: Instant,
    ) -> Result<(), AdminRangeError> {
        let region = self.range_region(region_id)?;
        let parsed = match policy.to_uppercase().as_str() {
            "SCAN" => SplitPolicy::Scan,
            "APPROXIMATE" => SplitPolicy::Approximate,
            "USEKEY" => SplitPolicy::UseKey,
            _ => return Err(AdminRangeError::UnsupportedPolicy(policy.into())),
        };
        let mut decoded = Vec::new();
        if parsed == SplitPolicy::UseKey {
            for key in keys {
                let decode = || {
                    if key.len() % 2 != 0 {
                        return None;
                    }
                    key.as_bytes()
                        .chunks_exact(2)
                        .map(|pair| {
                            let digit = |b: u8| match b {
                                b'0'..=b'9' => Some(b - b'0'),
                                b'a'..=b'f' => Some(b - b'a' + 10),
                                b'A'..=b'F' => Some(b - b'A' + 10),
                                _ => None,
                            };
                            Some(digit(pair[0])? * 16 + digit(pair[1])?)
                        })
                        .collect::<Option<Vec<_>>>()
                };
                decoded.push(decode().ok_or_else(|| AdminRangeError::InvalidHexKey(key.clone()))?);
            }
        }
        let op = create_split_region_operator(
            "admin-split-region",
            &region,
            OpKind::ADMIN,
            parsed,
            decoded,
        )?;
        self.admit_range(vec![Arc::new(op)], now)
    }
}

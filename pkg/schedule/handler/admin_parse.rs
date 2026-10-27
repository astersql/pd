// Copyright 2026 AsterSQL.
//! Management input dispatch. No HTTP server, protobuf, or private runtime.
use crate::{
    AdminPeerContext, AdminPeerError, AdminPeerRequest, AdminRangeContext, AdminRangeError,
    Handler, HandlerError,
};
use astersql_pd_schedule::Coordinator;
use astersql_pd_schedule_config::{CheckerConfigProvider, SchedulerConfigProvider};
use astersql_pd_schedule_filter::RegionFilter;
use astersql_pd_schedule_operator::BuilderError;
use astersql_pd_schedule_placement::PeerRoleType;
use astersql_pd_schedule_scatter::{ScatterBatchError, ScatterBatchFailure, ScatterBatchRequest};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum CreationError {
    #[error("{0}")]
    Input(&'static str),
    #[error(transparent)]
    Handler(#[from] HandlerError),
    #[error(transparent)]
    Peer(#[from] AdminPeerError),
    #[error(transparent)]
    Range(#[from] AdminRangeError),
    #[error("{0}")]
    Scatter(String),
    #[error(transparent)]
    ScatterBatch(#[from] ScatterBatchError),
    #[error(transparent)]
    ScatterFailure(#[from] ScatterBatchFailure),
}
/// Compatible status/body shape; adapters serialize the error separately.
#[derive(Debug)]
pub struct CreationResponse {
    pub status: u16,
    pub body: Value,
    pub error: Option<CreationError>,
}
impl CreationResponse {
    fn error(error: CreationError) -> Self {
        Self {
            status: if matches!(error, CreationError::Input(_)) {
                400
            } else {
                500
            },
            body: Value::Null,
            error: Some(error),
        }
    }
}
/// Complete parsed intent. Parsing never mutates cluster state.
#[derive(Clone, Debug)]
pub enum OperatorCreation {
    Peer {
        region: u64,
        request: AdminPeerRequest,
    },
    Merge {
        source: u64,
        target: u64,
    },
    Split {
        region: u64,
        policy: String,
        keys: Vec<String>,
    },
    Scatter {
        region: u64,
        group: String,
    },
    ScatterRegions {
        ids: Vec<u64>,
        start: String,
        end: String,
        group: String,
        retries: i64,
    },
}
fn id(v: &Value) -> Option<u64> {
    let n = v.as_f64()?;
    (n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n < 18446744073709551616.0)
        .then_some(n as u64)
}
fn ids(v: &Value) -> Option<Vec<u64>> {
    v.as_array()?.iter().map(id).collect()
}
impl OperatorCreation {
    pub fn parse(input: &Value) -> Result<Self, CreationError> {
        use CreationError::Input;
        let name = input["name"]
            .as_str()
            .ok_or(Input("missing operator name"))?;
        let field = |key: &str, error| id(&input[key]).ok_or(Input(error));
        let string = |key: &str| input[key].as_str().unwrap_or_default().to_owned();
        if name == "scatter-regions" {
            let ids = ids(&input["region_ids"]).ok_or(Input("region_ids is invalid"))?;
            let retries = match input["retry_limit"].as_f64() {
                Some(n) if n.is_finite() && n >= i64::MIN as f64 && n < 9223372036854775808.0 => {
                    n as i64
                }
                Some(_) => return Err(Input("retry_limit is invalid")),
                None => 5,
            };
            return Ok(Self::ScatterRegions {
                ids,
                start: string("start_key"),
                end: string("end_key"),
                group: string("group"),
                retries,
            });
        }
        if !matches!(
            name,
            "transfer-leader"
                | "transfer-region"
                | "transfer-peer"
                | "add-peer"
                | "add-learner"
                | "remove-peer"
                | "merge-region"
                | "split-region"
                | "scatter-region"
        ) {
            return Err(Input("unknown operator"));
        }
        if name == "merge-region" {
            return Ok(Self::Merge {
                source: field("source_region_id", "missing region id")?,
                target: field("target_region_id", "invalid target region id to merge to")?,
            });
        }
        let region = field("region_id", "missing region id")?;
        let request = match name {
            "transfer-leader" => AdminPeerRequest::TransferLeader(field(
                "to_store_id",
                "missing store id to transfer leader to",
            )?),
            "transfer-peer" => AdminPeerRequest::TransferPeer {
                from: field("from_store_id", "invalid store id to transfer peer from")?,
                to: field("to_store_id", "invalid store id to transfer peer to")?,
            },
            "add-peer" => AdminPeerRequest::AddPeer(field(
                "store_id",
                "invalid store id to transfer peer to",
            )?),
            "add-learner" => AdminPeerRequest::AddLearner(field(
                "store_id",
                "invalid store id to transfer peer to",
            )?),
            "remove-peer" => AdminPeerRequest::RemovePeer(field(
                "store_id",
                "invalid store id to transfer peer to",
            )?),
            "transfer-region" => {
                let ids = ids(&input["to_store_ids"])
                    .ok_or(Input("invalid store ids to transfer region to"))?;
                if ids.is_empty() {
                    return Err(Input("missing store ids to transfer region to"));
                }
                let roles = input["peer_roles"]
                    .as_array()
                    .filter(|r| r.len() == ids.len());
                let mut map = std::collections::BTreeMap::new();
                for id in &ids {
                    map.insert(*id, PeerRoleType::Unknown(String::new()));
                }
                if let Some(roles) = roles {
                    for (id, role) in ids.iter().zip(roles) {
                        if let Some(role) = role.as_str() {
                            map.insert(*id, PeerRoleType::from(role.to_owned()));
                        }
                    }
                }
                AdminPeerRequest::TransferRegion(map.into_iter().collect())
            }
            "split-region" => {
                let policy = input["policy"]
                    .as_str()
                    .ok_or(Input("missing split policy"))?
                    .into();
                let keys = match input.get("keys") {
                    None => vec![],
                    Some(v) => v
                        .as_array()
                        .ok_or(Input("bad format keys"))?
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .ok_or(Input("bad format keys"))
                        })
                        .collect::<Result<_, _>>()?,
                };
                return Ok(Self::Split {
                    region,
                    policy,
                    keys,
                });
            }
            "scatter-region" => {
                return Ok(Self::Scatter {
                    region,
                    group: string("group"),
                });
            }
            _ => unreachable!(),
        };
        Ok(Self::Peer { region, request })
    }
}
/// Borrow host-owned placement, hot/affinity snapshots, time and cancellation.
pub struct CreationContext<'a> {
    pub range: AdminRangeContext<'a>,
    pub hot: &'a BTreeSet<u64>,
    pub affinity: Option<&'a dyn RegionFilter>,
    pub cancellation: &'a CancellationToken,
}
/// Parse before bootstrapping checks, execute canonical actions, retain batch
/// scatter's HTTP-200 body even when the operation fails. Mutable coordinator
/// access is required by its canonical scatter distribution owner.
pub async fn handle_operator_creation<C, F>(
    coordinator: Option<&mut Coordinator<C>>,
    input: &Value,
    context: CreationContext<'_>,
    mut allocate: F,
) -> CreationResponse
where
    C: CheckerConfigProvider + SchedulerConfigProvider + Send + Sync + 'static,
    F: FnMut() -> Result<u64, BuilderError>,
{
    let action = match OperatorCreation::parse(input) {
        Ok(a) => a,
        Err(e) => return CreationResponse::error(e),
    };
    let batch = matches!(action, OperatorCreation::ScatterRegions { .. });
    let result: Result<Value, CreationError> = async {
        let c = coordinator.ok_or(HandlerError::NotBootstrapped)?;
        let h = Handler::new(Some(&*c));
        match action {
            OperatorCreation::Peer { region, request } => {
                let source = c.get_cluster().regions.get_region(region);
                let rules = source
                    .as_ref()
                    .map(|r| context.range.manager.get_rules_for_apply_region(r))
                    .unwrap_or_default();
                h.add_peer_operator(
                    region,
                    request,
                    AdminPeerContext {
                        rules: &rules,
                        now_unix_nanos: context.range.now_unix_nanos,
                        supports_conf_change_v2: context.range.supports_conf_change_v2,
                        now: context.range.now,
                    },
                    allocate,
                )
                .await?;
            }
            OperatorCreation::Merge { source, target } => {
                h.add_merge_region_operator(source, target, context.range, allocate)
                    .await?
            }
            OperatorCreation::Split {
                region,
                policy,
                keys,
            } => h.add_split_region_operator(region, &policy, &keys, context.range.now)?,
            OperatorCreation::Scatter { region, group } => {
                let source = c
                    .get_cluster()
                    .regions
                    .get_region(region)
                    .ok_or(AdminRangeError::RegionNotFound(region))?;
                if context.hot.contains(&region) {
                    return Err(CreationError::Scatter(format!(
                        "region {region} is a hot region"
                    )));
                }
                let mut scatter = c.get_region_scatterer(
                    context.range.manager,
                    context.hot,
                    context.affinity,
                    context.range.now_unix_nanos,
                    context.range.supports_conf_change_v2,
                );
                let mut suspect = BTreeSet::new();
                let op = scatter
                    .batch
                    .scatter_one(&source, &group, false, &mut suspect, &mut allocate)
                    .await;
                drop(scatter);
                c.get_checker_controller()
                    .lock()
                    .unwrap()
                    .add_pending_processed_regions(false, &suspect.into_iter().collect::<Vec<_>>());
                let op = op?;
                if let Some(op) = op {
                    let removed = c
                        .get_operator_controller()
                        .add_limited_operators(c.get_cluster(), &**c.get_config(), &[Arc::new(op)])
                        .await
                        .map_err(AdminRangeError::Admission)?
                        .removed;
                    c.get_operator_records()
                        .bury_removed(&removed, context.range.now);
                }
            }
            OperatorCreation::ScatterRegions {
                ids,
                start,
                end,
                group,
                retries,
            } => {
                let request = if !start.is_empty() && !end.is_empty() {
                    ScatterBatchRequest::Range {
                        start: decode_hex(&start)?,
                        end: decode_hex(&end)?,
                    }
                } else {
                    ScatterBatchRequest::Ids(ids)
                };
                let mut scatter = c.get_region_scatterer(
                    context.range.manager,
                    context.hot,
                    context.affinity,
                    context.range.now_unix_nanos,
                    context.range.supports_conf_change_v2,
                );
                let outcome = scatter
                    .scatter(
                        request,
                        &group,
                        retries.clamp(-1, 30) as i32,
                        false,
                        context.cancellation,
                        allocate,
                    )
                    .await?;
                let failures = outcome.failures.len();
                let percentage = if failures == 0 {
                    100
                } else {
                    100 - 100 * failures / (outcome.processed + failures)
                };
                return Ok(json!({"processed-percentage":percentage,"error":""}));
            }
        }
        Ok(Value::Null)
    }
    .await;
    match result {
        Ok(body) => CreationResponse {
            status: 200,
            body,
            error: None,
        },
        Err(e) if batch => CreationResponse {
            status: 200,
            body: json!({"processed-percentage":0,"error":e.to_string()}),
            error: None,
        },
        Err(e) => CreationResponse::error(e),
    }
}
pub(super) fn decode_hex(s: &str) -> Result<Vec<u8>, CreationError> {
    let digit = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    // Go encoding/hex reports the first invalid byte before odd length.
    for &b in s.as_bytes() {
        if digit(b).is_none() {
            let mut message = format!("encoding/hex: invalid byte: U+{b:04X}");
            if (32..=126).contains(&b) || (b >= 161 && b != 173) {
                let quoted = match b {
                    39 => "\\'".into(),
                    92 => "\\\\".into(),
                    _ => (b as char).to_string(),
                };
                message.push_str(&format!(" '{quoted}'"));
            }
            return Err(CreationError::Scatter(message));
        }
    }
    if s.len() % 2 != 0 {
        return Err(CreationError::Scatter(
            "encoding/hex: odd length hex string".into(),
        ));
    }
    Ok(s.as_bytes()
        .chunks_exact(2)
        .map(|p| digit(p[0]).unwrap() * 16 + digit(p[1]).unwrap())
        .collect())
}

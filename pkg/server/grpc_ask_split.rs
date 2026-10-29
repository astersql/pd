// Copyright 2026 AsterSQL.
//! Split requests use the host's canonical validation and fenced ID allocator.
use crate::grpc::Grpc;
use astersql_pd_core::RegionValidationError;
use astersql_pd_proto::{metapb, pdpb};
use tonic::{Response, Status};

/// Host errors retain domain validation data for protocol error formatting.
#[derive(Debug)]
pub enum SplitError {
    Validation(RegionValidationError),
    Other(String),
}
/// One result region and its peers, in request order.
pub type SplitIds = Vec<(u64, Vec<u64>)>;
impl Grpc {
    pub(crate) async fn split_request(
        &self,
        header: Option<&pdpb::RequestHeader>,
        region: Option<metapb::Region>,
        count: u32,
        batch: bool,
        reason: i32,
    ) -> Result<(pdpb::ResponseHeader, SplitIds), Status> {
        // Go AskBatchSplit checks bootstrap before role validation.
        if batch && let Some(header) = self.bootstrap_header().await? {
            return Ok((header, vec![]));
        }
        self.check(header).await?;
        if !batch && let Some(header) = self.bootstrap_header().await? {
            return Ok((header, vec![]));
        }
        let failure = |kind: pdpb::ErrorType, message| {
            (
                pdpb::ResponseHeader {
                    cluster_id: self.services.identity().cluster_id,
                    error: Some(pdpb::Error {
                        r#type: kind as i32,
                        message,
                    }),
                    ..Default::default()
                },
                vec![],
            )
        };
        if batch {
            let version = tokio::select! {biased;
                _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
                result=self.services.batch_split_version()=>result.map_err(Status::unavailable)?,
            };
            if let Some(version) = version {
                return Ok(failure(
                    pdpb::ErrorType::IncompatibleVersion,
                    format!("batch_split incompatible with current cluster version {version}"),
                ));
            }
        }
        let Some(region) = region else {
            return Ok(failure(
                pdpb::ErrorType::RegionNotFound,
                "missing region for split".into(),
            ));
        };
        let epoch_present = region.region_epoch.is_some();
        let result = tokio::select! {biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=self.services.split_ids(crate::core_region_adapter::region_from_proto(Some(region)).unwrap(),count,batch,reason)=>result,
        };
        Ok(match result {
            Ok(ids) => (self.header().unwrap(), ids),
            Err(error) => failure(
                pdpb::ErrorType::Unknown,
                match error {
                    SplitError::Other(message) => message,
                    SplitError::Validation(RegionValidationError::InvalidEpoch {
                        request,
                        current,
                    }) => format!(
                        "invalid region epoch, request: {}, current: {}",
                        if epoch_present {
                            epoch_text(request)
                        } else {
                            "<nil>".into()
                        },
                        epoch_text(current)
                    ),
                    SplitError::Validation(RegionValidationError::NotFound { request }) => format!(
                        "region not found, request region: {}",
                        region_text(&request)
                    ),
                },
            ),
        })
    }
    pub(crate) async fn ask_split_request(
        &self,
        r: pdpb::AskSplitRequest,
    ) -> Result<Response<pdpb::AskSplitResponse>, Status> {
        let (header, ids) = self
            .split_request(r.header.as_ref(), r.region, 1, false, 0)
            .await?;
        let (new_region_id, new_peer_ids) = ids.into_iter().next().unwrap_or_default();
        Ok(Response::new(pdpb::AskSplitResponse {
            header: Some(header),
            new_region_id,
            new_peer_ids,
        }))
    }
    pub(crate) async fn ask_batch_split_request(
        &self,
        r: pdpb::AskBatchSplitRequest,
    ) -> Result<Response<pdpb::AskBatchSplitResponse>, Status> {
        let (header, ids) = self
            .split_request(r.header.as_ref(), r.region, r.split_count, true, r.reason)
            .await?;
        Ok(Response::new(pdpb::AskBatchSplitResponse {
            header: Some(header),
            ids: ids
                .into_iter()
                .map(|(new_region_id, new_peer_ids)| pdpb::SplitId {
                    new_region_id,
                    new_peer_ids,
                })
                .collect(),
        }))
    }
}
fn epoch_text(e: astersql_pd_core::RegionEpoch) -> String {
    let mut s = String::new();
    if e.conf_ver != 0 {
        s += &format!("conf_ver:{} ", e.conf_ver);
    }
    if e.version != 0 {
        s += &format!("version:{} ", e.version);
    }
    s
}
fn region_text(r: &astersql_pd_core::RegionMeta) -> String {
    // Go's RegionToHexMeta uses uppercase hexadecimal key strings.
    let mut s = String::new();
    if r.id != 0 {
        s += &format!("id:{} ", r.id);
    }
    for (name, key) in [
        ("start_key", &r.key_range.start_key),
        ("end_key", &r.key_range.end_key),
    ] {
        if !key.is_empty() {
            s += &format!(
                "{name}:\"{}\" ",
                key.iter().map(|b| format!("{b:02X}")).collect::<String>()
            );
        }
    }
    if let Some(e) = r.epoch {
        s += &format!("region_epoch:<{}> ", epoch_text(e));
    }
    for p in &r.peers {
        let mut peer = String::new();
        if p.id != 0 {
            peer += &format!("id:{} ", p.id);
        }
        if p.store_id != 0 {
            peer += &format!("store_id:{} ", p.store_id);
        }
        use astersql_pd_core::PeerRole;
        let role = match p.role {
            PeerRole::Voter => None,
            PeerRole::Learner => Some("Learner".into()),
            PeerRole::IncomingVoter => Some("IncomingVoter".into()),
            PeerRole::DemotingVoter => Some("DemotingVoter".into()),
            PeerRole::Unknown(n) => Some(n.to_string()),
        };
        if let Some(role) = role {
            // The pinned Go gogo text path writes unregistered enum values twice.
            // Preserve its observed error text (e.g. LearnerLearner).
            peer += &format!("role:{role}{role} ");
        }
        if p.is_witness {
            peer += "is_witness:true ";
        }
        s += &format!("peers:<{peer}> ");
    }
    if let Some(e) = &r.encryption_meta {
        let mut value = String::new();
        if e.key_id != 0 {
            value += &format!("key_id:{} ", e.key_id);
        }
        if !e.iv.is_empty() {
            value += &format!("iv:{} ", quote_bytes(&e.iv));
        }
        s += &format!("encryption_meta:<{value}> ");
    }
    if r.is_in_flashback {
        s += "is_in_flashback:true ";
    }
    if r.flashback_start_ts != 0 {
        s += &format!("flashback_start_ts:{} ", r.flashback_start_ts);
    }
    s.trim_end().into()
}
fn quote_bytes(bytes: &[u8]) -> String {
    let mut s = String::from("\"");
    for &b in bytes {
        match b {
            b'\n' => s += "\\n",
            b'\r' => s += "\\r",
            b'\t' => s += "\\t",
            b'\\' => s += "\\\\",
            b'"' => s += "\\\"",
            32..=126 => s.push(b as char),
            _ => s += &format!("\\{b:03o}"),
        }
    }
    s.push('"');
    s
}

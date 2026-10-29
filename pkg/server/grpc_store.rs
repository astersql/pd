// Copyright 2026 AsterSQL.
//! Store registration protocol mapping; canonical state belongs to the host.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use tonic::{Response, Status};

/// Registration errors with distinct classic PD header codes.
#[derive(Debug)]
pub enum StoreError {
    Removed,
    InvalidStore(Box<astersql_pd_core::StoreMeta>),
    AddressConflict {
        peer: bool,
        incoming: Box<astersql_pd_core::StoreMeta>,
        existing: Box<astersql_pd_core::StoreMeta>,
    },
    InvalidEngine(String),
    Other(String),
}
impl Grpc {
    pub(crate) async fn put_store_request(
        &self,
        request: pdpb::PutStoreRequest,
    ) -> Result<Response<pdpb::PutStoreResponse>, Status> {
        self.check(request.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::PutStoreResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let missing = request.store.is_none();
        let store = crate::core_store_adapter::store_from_proto(request.store).unwrap_or_default();
        let result = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = async {
                if missing { Err(StoreError::Other("invalid put store <nil>".into())) }
                else { self.services.put_store(store).await }
            } => result,
        };
        let header = match result {
            Ok(()) => self.header(),
            Err(error) => {
                let (kind, message) = match error {
                    StoreError::AddressConflict {
                        peer,
                        incoming,
                        existing,
                    } => (
                        pdpb::ErrorType::Unknown,
                        format!(
                            "duplicated store {}address: {}, already registered by {}",
                            if peer { "peer " } else { "" },
                            store_text(*incoming),
                            store_text(*existing)
                        ),
                    ),
                    StoreError::InvalidStore(store) => (
                        pdpb::ErrorType::Unknown,
                        format!("invalid put store {}", store_text(*store)),
                    ),
                    StoreError::Removed => {
                        (pdpb::ErrorType::StoreTombstone, "store is tombstone".into())
                    }
                    StoreError::InvalidEngine(message) => (pdpb::ErrorType::InvalidValue, message),
                    StoreError::Other(message) => (pdpb::ErrorType::Unknown, message),
                };
                Some(pdpb::ResponseHeader {
                    cluster_id: self.services.identity().cluster_id,
                    error: Some(pdpb::Error {
                        r#type: kind as i32,
                        message,
                    }),
                    ..Default::default()
                })
            }
        };
        Ok(Response::new(pdpb::PutStoreResponse {
            replication_status: header.as_ref().filter(|h| h.error.is_none()).map(|_| {
                astersql_pd_proto::replication_modepb::ReplicationStatus {
                    mode: astersql_pd_proto::replication_modepb::ReplicationMode::Majority as i32,
                    ..Default::default()
                }
            }),
            header,
        }))
    }
    pub(crate) async fn all_stores_request(
        &self,
        request: pdpb::GetAllStoresRequest,
    ) -> Result<Response<pdpb::GetAllStoresResponse>, Status> {
        self.check(request.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::GetAllStoresResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let stores = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = self.services.stores() => result.map_err(Status::unavailable)?,
        };
        Ok(Response::new(pdpb::GetAllStoresResponse {
            header: self.header(),
            stores: stores
                .into_iter()
                .filter(|s| {
                    !request.exclude_tombstone_stores
                        || s.node_state != astersql_pd_core::NodeState::Removed
                })
                .map(crate::core_store_adapter::store_to_proto)
                .collect(),
        }))
    }
}

// Pinned metapb.Store compact text used by Go's registration error messages.
fn store_text(meta: astersql_pd_core::StoreMeta) -> String {
    use std::fmt::Write;
    let store = crate::core_store_adapter::store_to_proto(meta);
    fn quoted(text: &str) -> String {
        use std::fmt::Write;
        let mut result = String::from("\"");
        for b in text.bytes() {
            match b {
                b'\n' => result.push_str("\\n"),
                b'\r' => result.push_str("\\r"),
                b'\t' => result.push_str("\\t"),
                b'"' => result.push_str("\\\""),
                b'\\' => result.push_str("\\\\"),
                0x20..=0x7e => result.push(b as char),
                _ => write!(result, "\\{b:03o}").unwrap(),
            }
        }
        result.push('"');
        result
    }
    let mut out = String::new();
    macro_rules! number {
        ($name:ident) => {
            if store.$name != 0 {
                write!(out, "{}:{} ", stringify!($name), store.$name).unwrap();
            }
        };
    }
    macro_rules! text {
        ($name:ident) => {
            if !store.$name.is_empty() {
                write!(out, "{}:{} ", stringify!($name), quoted(&store.$name)).unwrap();
            }
        };
    }
    number!(id);
    text!(address);
    if store.state != 0 {
        let name = astersql_pd_proto::metapb::StoreState::try_from(store.state)
            .ok()
            .map(|s| s.as_str_name().to_owned())
            .unwrap_or_else(|| store.state.to_string());
        write!(out, "state:{name} ").unwrap();
    }
    for label in store.labels {
        out.push_str("labels:<");
        if !label.key.is_empty() {
            write!(out, "key:{} ", quoted(&label.key)).unwrap();
        }
        if !label.value.is_empty() {
            write!(out, "value:{} ", quoted(&label.value)).unwrap();
        }
        out.push_str("> ");
    }
    text!(version);
    text!(peer_address);
    text!(status_address);
    text!(git_hash);
    number!(start_timestamp);
    text!(deploy_path);
    number!(last_heartbeat);
    if store.physically_destroyed {
        out.push_str("physically_destroyed:true ");
    }
    if store.node_state != 0 {
        let name = astersql_pd_proto::metapb::NodeState::try_from(store.node_state)
            .ok()
            .map(|s| s.as_str_name().to_owned())
            .unwrap_or_else(|| store.node_state.to_string());
        write!(out, "node_state:{name} ").unwrap();
    }
    if let Some(range) = store.txn_protocol_version_range {
        out.push_str("txn_protocol_version_range:<");
        if range.min != 0 {
            write!(out, "min:{} ", range.min).unwrap();
        }
        if range.max != 0 {
            write!(out, "max:{} ", range.max).unwrap();
        }
        out.push_str("> ");
    }
    out
}

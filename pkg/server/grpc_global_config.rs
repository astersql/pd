// Copyright 2026 AsterSQL.
//! Global configuration uses the shared backend independently of raft bootstrap/leadership.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use astersql_pd_storage::{Consistency, Mutation, Read, StoreError};
use tonic::{Response, Status};

const ROOT: &str = "/global/config/";
const CONTROLLER: &str = astersql_pd_keypath::controller_config_path();

pub(crate) fn normalize(path: &str) -> Result<&str, Status> {
    if path.is_empty() || path == "/global/config" {
        Ok(ROOT)
    } else if path.starts_with(ROOT) {
        Ok(path)
    } else {
        Err(Status::invalid_argument(format!(
            "global config path {path:?} is not allowed"
        )))
    }
}

// etcd keys are literal bytes: do not clean dot segments, leading slashes or escapes.
fn key(path: &str, name: &str) -> Vec<u8> {
    if name.is_empty() {
        path.strip_suffix('/').unwrap_or(path).as_bytes().to_vec()
    } else if path.ends_with('/') {
        format!("{path}{name}").into_bytes()
    } else {
        format!("{path}/{name}").into_bytes()
    }
}

fn store_status(error: StoreError) -> Status {
    match error {
        StoreError::RequestTooLarge => Status::unknown("etcdserver: request is too large"),
        error => Status::unknown(error.to_string()),
    }
}

impl Grpc {
    pub(crate) async fn store_global_config_request(
        &self,
        request: pdpb::StoreGlobalConfigRequest,
    ) -> Result<Response<pdpb::StoreGlobalConfigResponse>, Status> {
        let store = self
            .services
            .discovery_store()
            .ok_or_else(|| Status::unavailable("server is started, but etcd not started"))?;
        let path = normalize(&request.config_path)?;
        let writes = request
            .changes
            .into_iter()
            .map(|item| {
                let key = key(path, &item.name);
                match pdpb::EventType::try_from(item.kind) {
                    Ok(pdpb::EventType::Put) => Ok(Mutation::put(
                        &key,
                        if item.value.is_empty() {
                            &item.payload
                        } else {
                            item.value.as_bytes()
                        },
                    )),
                    Ok(pdpb::EventType::Delete) => Ok(Mutation::Delete { key }),
                    Err(_) => Err(Status::invalid_argument("invalid global config event kind")),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let result = tokio::select! {biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = store.transact(vec![], writes) => result.map_err(store_status)?,
        };
        if !result.succeeded {
            return Err(Status::unknown(
                "failed to execute StoreGlobalConfig transaction",
            ));
        }
        Ok(Response::new(pdpb::StoreGlobalConfigResponse::default()))
    }

    pub(crate) async fn load_global_config_request(
        &self,
        request: pdpb::LoadGlobalConfigRequest,
    ) -> Result<Response<pdpb::LoadGlobalConfigResponse>, Status> {
        let store = self
            .services
            .discovery_store()
            .ok_or_else(|| Status::unavailable("server is started, but etcd not started"))?;
        // An absent/empty repeated field has the same wire representation in Go and prost.
        let path = if request.names.is_empty() && request.config_path == CONTROLLER {
            CONTROLLER
        } else {
            normalize(&request.config_path)?
        };
        let load = async {
            if request.names.is_empty() {
                let snapshot = store
                    .read(Read::prefix(path.as_bytes(), Consistency::Linearizable))
                    .await
                    .map_err(|error| Status::unknown(error.to_string()))?;
                return Ok(pdpb::LoadGlobalConfigResponse {
                    revision: snapshot.revision,
                    items: snapshot
                        .entries
                        .into_iter()
                        .map(|entry| pdpb::GlobalConfigItem {
                            name: String::from_utf8_lossy(&entry.key).into_owned(),
                            payload: entry.value,
                            kind: pdpb::EventType::Put as i32,
                            ..Default::default()
                        })
                        .collect(),
                });
            }
            let mut items = Vec::with_capacity(request.names.len());
            for name in request.names {
                let result = store
                    .read(Read::range(
                        &key(path, &name),
                        b"",
                        Consistency::Linearizable,
                    ))
                    .await;
                let mut item = pdpb::GlobalConfigItem {
                    name,
                    ..Default::default()
                };
                match result {
                    Err(error) => {
                        item.error = Some(pdpb::Error {
                            r#type: pdpb::ErrorType::Unknown as i32,
                            message: error.to_string(),
                        })
                    }
                    Ok(snapshot) => match snapshot.entries.into_iter().next() {
                        Some(entry) => item.payload = entry.value,
                        None => {
                            item.error = Some(pdpb::Error {
                                r#type: pdpb::ErrorType::GlobalConfigNotFound as i32,
                                message: format!("key {} not found", item.name),
                            })
                        }
                    },
                }
                items.push(item);
            }
            Ok(pdpb::LoadGlobalConfigResponse { items, revision: 0 })
        };
        tokio::select! {biased;
            _ = self.cancel.cancelled() => Err(Status::unavailable("server not started")),
            result = load => result.map(Response::new),
        }
    }
}

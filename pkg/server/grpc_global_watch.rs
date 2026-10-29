// Copyright 2026 AsterSQL.
//! Pull-driven global configuration watch; dropping the response owns all cleanup.
use crate::{grpc::Grpc, grpc_global_config::normalize};
use astersql_pd_proto::pdpb;
use astersql_pd_storage::{Consistency, Event, Read, StoreError};
use tonic::codegen::BoxStream;
use tonic::{Response, Status};

impl Grpc {
    pub(crate) async fn watch_global_config_request(
        &self,
        request: tonic::Request<pdpb::WatchGlobalConfigRequest>,
    ) -> Result<Response<BoxStream<pdpb::WatchGlobalConfigResponse>>, Status> {
        let deadline = request
            .metadata()
            .get("grpc-timeout")
            .and_then(|value| value.to_str().ok())
            .and_then(timeout)
            .and_then(|duration| tokio::time::Instant::now().checked_add(duration));
        let inner = self.watch_global_config_inner(request.into_inner());
        let Some(deadline) = deadline else {
            return inner.await;
        };
        let response = tokio::time::timeout_at(deadline, inner)
            .await
            .map_err(|_| Status::deadline_exceeded("context deadline exceeded"))??;
        use futures_util::StreamExt;
        let output = futures_util::stream::unfold(
            (response.into_inner(), false),
            move |(mut stream, done)| async move {
                if done {
                    return None;
                }
                match tokio::time::timeout_at(deadline, stream.next()).await {
                    Ok(Some(item)) => Some((item, (stream, false))),
                    Ok(None) => None,
                    Err(_) => Some((
                        Err(Status::deadline_exceeded("context deadline exceeded")),
                        (stream, true),
                    )),
                }
            },
        );
        Ok(Response::new(Box::pin(output)))
    }

    async fn watch_global_config_inner(
        &self,
        request: pdpb::WatchGlobalConfigRequest,
    ) -> Result<Response<BoxStream<pdpb::WatchGlobalConfigResponse>>, Status> {
        let store = self
            .services
            .discovery_store()
            .ok_or_else(|| Status::unavailable("server is started, but etcd not started"))?;
        let path = normalize(&request.config_path)?.as_bytes().to_vec();
        let cancel = self.cancel.clone();
        let cluster_id = self.services.identity().cluster_id;
        let revision = if request.revision <= 0 {
            tokio::select! { biased;
                _ = cancel.cancelled() => return Err(Status::unavailable("server not started")),
                snapshot = store.read(Read::prefix(&path, Consistency::Linearizable)) =>
                    snapshot.map_err(|e| Status::unknown(e.to_string()))?.revision + 1,
            }
        } else {
            request.revision
        };
        let watch = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(Status::unavailable("server not started")),
            watch = store.watch_with_previous(path, revision) => watch.map_err(|e| Status::unknown(e.to_string()))?,
        };
        // There is no spawned producer or unbounded queue. Tonic owns this stream and
        // drops the backend RPC on client disconnect, deadline, or host shutdown.
        let output = futures_util::stream::unfold(
            (watch, cancel, revision, None, false),
            move |(mut watch, cancel, mut revision, pending, terminal)| async move {
                if terminal {
                    return None;
                }
                if let Some(status) = pending {
                    return Some((Err(status), (watch, cancel, revision, None, true)));
                }
                loop {
                    let result = tokio::select! { biased;
                        _ = cancel.cancelled() => return None,
                        result = watch.next(&cancel) => result,
                    };
                    match result {
                        Ok(batch) => {
                            let changes = batch
                                .events
                                .iter()
                                .filter_map(|event| {
                                    let (kind, payload) = match event {
                                        Event::Put(entry) => {
                                            (pdpb::EventType::Put, entry.value.clone())
                                        }
                                        Event::Delete { .. } => {
                                            (pdpb::EventType::Delete, watch.previous_value(event)?)
                                        }
                                    };
                                    Some(pdpb::GlobalConfigItem {
                                        name: String::from_utf8_lossy(event.key()).into_owned(),
                                        payload,
                                        kind: kind as i32,
                                        ..Default::default()
                                    })
                                })
                                .collect::<Vec<_>>();
                            revision = batch.revision;
                            if !changes.is_empty() {
                                let response = pdpb::WatchGlobalConfigResponse {
                                    changes,
                                    revision,
                                    ..Default::default()
                                };
                                return Some((
                                    Ok(response),
                                    (watch, cancel, revision, None, false),
                                ));
                            }
                        }
                        Err(StoreError::Unavailable) => {
                            // MetaWatch resumes from the last delivered cursor. Yield during
                            // outages, and let cancellation interrupt the bounded backoff.
                            tokio::select! { biased;
                                _ = cancel.cancelled() => return None,
                                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {},
                            }
                        }
                        Err(StoreError::Cancelled) => return None,
                        Err(error) => {
                            let (kind, message, status) = match error {
                                StoreError::Compacted {
                                    compact_revision: Some(boundary),
                                    ..
                                } if revision < boundary => (
                                    pdpb::ErrorType::DataCompacted,
                                    format!(
                                        "required watch revision: {revision} is smaller than current compact/min revision {boundary}."
                                    ),
                                    Status::unknown(
                                        "etcdserver: mvcc: required revision has been compacted",
                                    ),
                                ),
                                StoreError::Compacted { .. } => (
                                    pdpb::ErrorType::Unknown,
                                    "watch channel meet other error etcdserver: mvcc: required revision has been compacted.".into(),
                                    Status::unknown("etcdserver: mvcc: required revision has been compacted"),
                                ),
                                error => (
                                    pdpb::ErrorType::Unknown,
                                    format!("watch channel meet other error {}.", error),
                                    Status::unknown(error.to_string()),
                                ),
                            };
                            let response = pdpb::WatchGlobalConfigResponse {
                                header: Some(pdpb::ResponseHeader {
                                    cluster_id,
                                    error: Some(pdpb::Error {
                                        r#type: kind as i32,
                                        message,
                                    }),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            };
                            return Some((
                                Ok(response),
                                (watch, cancel, revision, Some(status), false),
                            ));
                        }
                    }
                }
            },
        );
        Ok(Response::new(Box::pin(output)))
    }
}

// gRPC timeout wire format: at most eight decimal digits and one unit suffix.
fn timeout(value: &str) -> Option<std::time::Duration> {
    if !(2..=9).contains(&value.len()) {
        return None;
    }
    let (digits, unit) = value.split_at(value.len() - 1);
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let count: u64 = digits.parse().ok()?;
    Some(match unit {
        "H" => std::time::Duration::from_secs(count * 3600),
        "M" => std::time::Duration::from_secs(count * 60),
        "S" => std::time::Duration::from_secs(count),
        "m" => std::time::Duration::from_millis(count),
        "u" => std::time::Duration::from_micros(count),
        "n" => std::time::Duration::from_nanos(count),
        _ => return None,
    })
}

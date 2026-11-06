// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::{Event, MetaWatch};
use futures_util::StreamExt;
use std::time::Duration;

impl Service {
    pub(crate) async fn watch_request(
        &self,
        request: p::WatchRequest,
    ) -> Result<tonic::codegen::BoxStream<p::WatchResponse>, Status> {
        let store = self.store().await?;
        if request.key.is_empty() {
            // The admitted Go client keeps this invalid subscription pending;
            // it emits neither headers nor an error frame before cancellation.
            self.cancel.cancelled().await;
            return Ok(Box::pin(futures_util::stream::empty()));
        }
        let watch = store
            .watch_range(
                request.key.clone(),
                request.range_end,
                request.start_revision,
                request.prev_kv,
            )
            .await;
        let (watch, initial_error) = match watch {
            Ok(w) => (Some(w), None),
            Err(e) => (None, Some(e)),
        };
        let mut state = WatchState {
            watch,
            initial_error,
            terminal: None,
            finished: false,
            cluster: self.host.cluster_id(),
            cancel: self.cancel.child_token(),
            previous: request.prev_kv,
            start: request.start_revision,
        };
        // The transport pulls directly from MetaWatch. Dropping the response drops
        // the backend stream immediately; there is no detached forwarding worker.
        // Go sends the initial HTTP/2 headers with its first watch message.
        // Do not acknowledge an idle subscription before the first event/error.
        let first = state.next().await;
        let remaining = futures_util::stream::unfold(state, |mut state| async move {
            let item = state.next().await?;
            Some((item, state))
        });
        Ok(Box::pin(futures_util::stream::iter(first).chain(remaining)))
    }
}
struct WatchState {
    watch: Option<Box<dyn MetaWatch>>,
    initial_error: Option<StoreError>,
    terminal: Option<Status>,
    finished: bool,
    cluster: u64,
    cancel: CancellationToken,
    previous: bool,
    start: i64,
}
impl WatchState {
    fn header(&self, revision: i64, error: Option<p::Error>) -> Option<p::ResponseHeader> {
        Some(p::ResponseHeader {
            cluster_id: self.cluster,
            revision,
            error,
        })
    }
    async fn next(&mut self) -> Option<Result<p::WatchResponse, Status>> {
        if self.finished || self.cancel.is_cancelled() {
            return None;
        }
        if let Some(error) = self.terminal.take() {
            self.finished = true;
            return Some(Err(error));
        }
        loop {
            let result = if let Some(error) = self.initial_error.take() {
                Err(error)
            } else {
                let watch = self.watch.as_mut()?;
                tokio::select! {biased;_=self.cancel.cancelled()=>return None,result=watch.next_with_progress(&self.cancel)=>result}
            };
            match result {
                Ok(batch) => {
                    if batch.events.is_empty() {
                        continue;
                    }
                    let watch = self.watch.as_ref().unwrap();
                    let events = batch
                        .events
                        .iter()
                        .map(|event| {
                            let (kind, kv) = match event {
                                Event::Put(e) => (
                                    0,
                                    p::KeyValue {
                                        key: e.key.clone(),
                                        value: e.value.clone(),
                                        create_revision: e.create_revision,
                                        mod_revision: e.mod_revision,
                                        version: e.version,
                                        lease: e.lease.unwrap_or(0),
                                    },
                                ),
                                Event::Delete { key, revision } => (
                                    1,
                                    p::KeyValue {
                                        key: key.clone(),
                                        mod_revision: *revision,
                                        ..Default::default()
                                    },
                                ),
                            };
                            let prev_kv = if self.previous {
                                watch.previous_value(event).map(|value| p::KeyValue {
                                    key: event.key().to_vec(),
                                    value,
                                    ..Default::default()
                                })
                            } else {
                                None
                            };
                            p::Event {
                                r#type: kind,
                                kv: Some(kv),
                                prev_kv,
                            }
                        })
                        .collect();
                    return Some(Ok(p::WatchResponse {
                        header: self
                            .header(watch.header_revision().unwrap_or(batch.revision), None),
                        compact_revision: 0,
                        events,
                    }));
                }
                Err(StoreError::Cancelled) => return None,
                Err(StoreError::Unavailable) if self.watch.is_some() => {
                    tokio::select! {biased;_=self.cancel.cancelled()=>return None,_=tokio::time::sleep(Duration::from_millis(100))=>{}}
                }
                Err(error) => {
                    let compact = match &error {
                        StoreError::Compacted {
                            compact_revision, ..
                        } => compact_revision.unwrap_or(0),
                        _ => 0,
                    };
                    let terminal = error_message(error);
                    let (kind, message) = if self.start < compact {
                        (
                            p::ErrorType::DataCompacted,
                            format!(
                                "required watch revision: {} is smaller than current compact/min revision {}.",
                                self.start, compact
                            ),
                        )
                    } else {
                        (
                            p::ErrorType::Unknown,
                            format!("watch channel meet other error {terminal}."),
                        )
                    };
                    self.terminal = Some(Status::unknown(terminal));
                    return Some(Ok(p::WatchResponse {
                        header: self.header(
                            0,
                            Some(p::Error {
                                r#type: kind as i32,
                                message,
                            }),
                        ),
                        compact_revision: compact,
                        events: vec![],
                    }));
                }
            }
        }
    }
}

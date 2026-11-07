// Copyright 2026 AsterSQL.
use std::sync::Weak;
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::native::Node;
use super::native_state::Change;
use crate::*;

pub(super) struct NativeWatch {
    node: Weak<Node>,
    prefix: Vec<u8>,
    next: Revision,
    header: Option<Revision>,
    last: Vec<Change>,
    cancelled: bool,
}

impl NativeWatch {
    pub(super) fn new(node: Weak<Node>, prefix: Vec<u8>, next: Revision) -> Self {
        Self {
            node,
            prefix,
            next,
            header: None,
            last: Vec::new(),
            cancelled: false,
        }
    }

    async fn pull(&mut self, cancel: &CancellationToken, progress: bool) -> Result<WatchBatch> {
        loop {
            if self.cancelled || cancel.is_cancelled() {
                self.cancelled = true;
                return Err(StoreError::Cancelled);
            }
            let node = self.node.upgrade().ok_or(StoreError::Unavailable)?;
            let mut changed = std::pin::pin!(node.machine.changed().notified());
            changed.as_mut().enable();
            tokio::select! {
                biased;
                _ = cancel.cancelled() => { self.cancelled = true; return Err(StoreError::Cancelled); },
                result = node.barrier() => result?,
            }
            let (header, changes) = node.inspect(|state| state.watch(&self.prefix, self.next))?;
            if let Some(last) = changes.last() {
                let revision = last.event.revision();
                self.next = revision.checked_add(1).ok_or(StoreError::Backend)?;
                self.header = Some(header);
                let events = changes.iter().map(|change| change.event.clone()).collect();
                self.last = changes;
                return Ok(WatchBatch { events, revision });
            }
            if progress && self.header != Some(header) && header >= self.next.saturating_sub(1) {
                self.next = header.checked_add(1).ok_or(StoreError::Backend)?;
                self.header = Some(header);
                self.last.clear();
                return Ok(WatchBatch {
                    events: Vec::new(),
                    revision: header,
                });
            }
            tokio::select! {
                biased;
                _ = cancel.cancelled() => { self.cancelled = true; return Err(StoreError::Cancelled); },
                _ = node.cancelled.cancelled() => return Err(StoreError::Unavailable),
                _ = &mut changed => {},
                _ = tokio::time::sleep(Duration::from_secs(1)) => {},
            }
        }
    }
}

#[async_trait]
impl MetaWatch for NativeWatch {
    fn header_revision(&self) -> Option<Revision> {
        self.header
    }

    fn previous_value(&self, event: &Event) -> Option<Vec<u8>> {
        if !matches!(event, Event::Delete { .. }) {
            return None;
        }
        self.last
            .iter()
            .find(|change| change.event == *event)?
            .previous
            .clone()
    }

    async fn next(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        self.pull(cancel, false).await
    }

    async fn next_with_progress(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        self.pull(cancel, true).await
    }
}

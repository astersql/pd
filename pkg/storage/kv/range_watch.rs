// Copyright 2026 AsterSQL.
use crate::*;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
/// Backend-neutral selection over an ordered, inclusive prefix watch.
pub(crate) struct RangeWatch {
    pub watch: Box<dyn MetaWatch>,
    pub key: Vec<u8>,
    pub end: Vec<u8>,
}
impl RangeWatch {
    fn contains(&self, key: &[u8]) -> bool {
        if self.end.is_empty() {
            key == self.key
        } else {
            key >= self.key.as_slice() && (self.end == [0] || key < self.end.as_slice())
        }
    }
}
#[async_trait]
impl MetaWatch for RangeWatch {
    fn header_revision(&self) -> Option<Revision> {
        self.watch.header_revision()
    }
    fn previous_value(&self, event: &Event) -> Option<Vec<u8>> {
        self.watch.previous_value(event)
    }
    async fn next(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        loop {
            let mut batch = self.watch.next(cancel).await?;
            batch.events.retain(|event| self.contains(event.key()));
            if !batch.events.is_empty() {
                return Ok(batch);
            }
        }
    }
    async fn next_with_progress(&mut self, cancel: &CancellationToken) -> Result<WatchBatch> {
        let mut batch = self.watch.next_with_progress(cancel).await?;
        batch.events.retain(|event| self.contains(event.key()));
        Ok(batch)
    }
}

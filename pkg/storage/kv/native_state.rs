// Copyright 2026 AsterSQL.
use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD};
use openraft::StorageError;
use serde::{Deserialize, Serialize};

use super::candidate_raft::ReplayApplication;
use super::native::NativeRaft;
use super::native_backup::RecoveryImage;
use crate::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NativeCommand {
    Transaction {
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    },
    Compact(Revision),
    Grant(i64),
    Renew(LeaseId),
    Revoke(LeaseId),
    /// Monotonic elapsed lease time proposed by the current leader.
    AdvanceClock(u64),
    Restore(RecoveryImage),
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub enum NativeReply {
    #[default]
    Empty,
    Transaction(Result<RawTxnResponse>),
    Lease(Result<Lease>),
    Administrative(Result<()>),
}

#[derive(Deserialize, Serialize)]
pub(super) struct Application {
    native: State,
}

impl Default for Application {
    fn default() -> Self {
        Self {
            native: State::default(),
        }
    }
}

impl Application {
    pub(super) fn state(&self) -> &State {
        &self.native
    }
}

impl ReplayApplication<NativeRaft> for Application {
    fn apply(
        &mut self,
        command: NativeCommand,
    ) -> std::result::Result<NativeReply, StorageError<u64>> {
        let s = &mut self.native;
        Ok(match command {
            NativeCommand::Transaction {
                compares,
                then_ops,
                else_ops,
            } => NativeReply::Transaction(s.transact(compares, then_ops, else_ops)),
            NativeCommand::Compact(revision) => NativeReply::Administrative(s.compact(revision)),
            NativeCommand::Grant(ttl) => NativeReply::Lease(s.grant(ttl)),
            NativeCommand::Renew(id) => NativeReply::Lease(s.renew(id)),
            NativeCommand::Revoke(id) => NativeReply::Administrative(s.remove_lease(id)),
            NativeCommand::AdvanceClock(time) => NativeReply::Administrative(s.advance_clock(time)),
            NativeCommand::Restore(image) => NativeReply::Administrative(s.restore(image)),
        })
    }
}

#[derive(Deserialize, Serialize)]
struct Version {
    revision: Revision,
    entry: Option<Entry>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Change {
    pub(super) event: Event,
    pub(super) previous: Option<Vec<u8>>,
}

#[derive(Deserialize, Serialize)]
struct LeaseDeadline {
    ttl_seconds: i64,
    expires: u64,
}

#[derive(Deserialize, Serialize)]
pub(super) struct State {
    revision: Revision,
    compacted: Revision,
    #[serde(with = "byte_keys")]
    keys: BTreeMap<Vec<u8>, Vec<Version>>,
    #[serde(deserialize_with = "integer_keys")]
    changes: BTreeMap<Revision, Vec<Change>>,
    #[serde(deserialize_with = "integer_keys")]
    leases: BTreeMap<LeaseId, LeaseDeadline>,
    next_lease: LeaseId,
    lease_clock: u64,
}

// The replay envelope flattens the application for compatibility with the
// original snapshot. Its buffered deserializer treats JSON object keys as strings.
fn integer_keys<'de, D, T>(deserializer: D) -> std::result::Result<BTreeMap<i64, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    BTreeMap::<String, T>::deserialize(deserializer)?
        .into_iter()
        .map(|(key, value)| {
            let key = key.parse().map_err(serde::de::Error::custom)?;
            Ok((key, value))
        })
        .collect()
}

mod byte_keys {
    use super::*;

    pub(super) fn serialize<S: serde::Serializer>(
        map: &BTreeMap<Vec<u8>, Vec<Version>>,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut out = serializer.serialize_map(Some(map.len()))?;
        for (key, history) in map {
            out.serialize_entry(&STANDARD.encode(key), history)?;
        }
        out.end()
    }

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<BTreeMap<Vec<u8>, Vec<Version>>, D::Error> {
        let map = BTreeMap::<String, Vec<Version>>::deserialize(deserializer)?;
        map.into_iter()
            .map(|(key, history)| {
                let key = STANDARD.decode(key).map_err(serde::de::Error::custom)?;
                Ok((key, history))
            })
            .collect()
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            revision: 1,
            compacted: 0,
            keys: BTreeMap::new(),
            changes: BTreeMap::new(),
            leases: BTreeMap::new(),
            next_lease: 1,
            lease_clock: 0,
        }
    }
}

impl State {
    pub(super) fn backup(&self, source_cluster: u64) -> RecoveryImage {
        RecoveryImage {
            source_cluster,
            revision: self.revision,
            next_lease: self.next_lease,
            entries: self
                .keys
                .values()
                .filter_map(|versions| versions.last()?.entry.clone())
                .collect(),
        }
    }

    fn restore(&mut self, image: RecoveryImage) -> Result<()> {
        if self.revision != 1 || self.next_lease != 1 || self.compacted != 0 {
            return Err(StoreError::InvalidArgument);
        }
        image.validate()?;
        self.revision = image.revision + 1;
        self.compacted = self.revision;
        self.next_lease = image.next_lease;
        for entry in image
            .entries
            .into_iter()
            .filter(|entry| entry.lease.is_none())
        {
            self.keys.insert(
                entry.key.clone(),
                vec![Version {
                    revision: self.revision,
                    entry: Some(entry),
                }],
            );
        }
        Ok(())
    }

    fn entry(&self, key: &[u8]) -> Option<&Entry> {
        self.keys.get(key)?.last()?.entry.as_ref()
    }

    fn scan<'a>(
        &'a self,
        request: &'a Read,
        revision: Revision,
    ) -> impl Iterator<Item = &'a Entry> {
        let start = request.start.as_ref().unwrap_or(&request.prefix);
        self.keys
            .range(start.clone()..)
            .take_while(move |(key, _)| match &request.end {
                Some(end) if end.is_empty() => *key == start,
                Some(end) => *key < end,
                None => key.starts_with(&request.prefix),
            })
            .filter_map(move |(_, history)| {
                let count = history.partition_point(|v| v.revision <= revision);
                history.get(count.checked_sub(1)?)?.entry.as_ref()
            })
    }

    fn check_compacted(&self, requested: Revision) -> Result<()> {
        if requested < self.compacted {
            return Err(StoreError::Compacted {
                requested,
                compact_revision: Some(self.compacted),
            });
        }
        Ok(())
    }

    pub(super) fn read(&self, request: &Read) -> Result<Snapshot> {
        let revision = request.revision.unwrap_or(self.revision);
        self.check_compacted(revision)?;
        if revision > self.revision {
            return Err(StoreError::FutureRevision);
        }
        let mut entries = self.scan(request, revision);
        let limit = if request.limit == 0 {
            usize::MAX
        } else {
            request.limit
        };
        Ok(Snapshot {
            revision: self.revision,
            entries: entries.by_ref().take(limit).cloned().collect(),
            more: entries.next().is_some(),
        })
    }

    pub(super) fn watch(&self, prefix: &[u8], next: Revision) -> Result<(Revision, Vec<Change>)> {
        self.check_compacted(next)?;
        let batch = self.changes.range(next..).find_map(|(_, changes)| {
            let matches: Vec<_> = changes
                .iter()
                .filter(|change| change.event.key().starts_with(prefix))
                .cloned()
                .collect();
            (!matches.is_empty()).then_some(matches)
        });
        Ok((self.revision, batch.unwrap_or_default()))
    }

    fn compare(&self, cmp: &Compare) -> bool {
        match cmp {
            Compare::CreateRevision(k, r) => self.entry(k).map_or(0, |e| e.create_revision) == *r,
            Compare::ModRevision(k, r) => self.entry(k).map_or(0, |e| e.mod_revision) == *r,
            Compare::Value(k, v) => self.entry(k).is_some_and(|e| e.value == *v),
            Compare::ValueNotEqual(k, v) => self.entry(k).is_some_and(|e| e.value != *v),
            Compare::ValueLess(k, v) => self.entry(k).is_some_and(|e| e.value < *v),
            Compare::ValueGreater(k, v) => self.entry(k).is_some_and(|e| e.value > *v),
            Compare::Exists(k) => self.entry(k).is_some(),
            Compare::NotExists(k) => self.entry(k).is_none(),
            Compare::Lease(k, id) => self.entry(k).and_then(|e| e.lease).unwrap_or(0) == *id,
        }
    }

    fn transact(
        &mut self,
        compares: Vec<Compare>,
        then_ops: Vec<RawTxnOp>,
        else_ops: Vec<RawTxnOp>,
    ) -> Result<RawTxnResponse> {
        raw_get::validate_raw(&compares, &then_ops)?;
        raw_get::validate_raw(&compares, &else_ops)?;
        let succeeded = compares.iter().all(|c| self.compare(c));
        let ops = if succeeded { then_ops } else { else_ops };
        let mut changes = false;
        for op in &ops {
            match op {
                RawTxnOp::Write(Mutation::Put { key, lease, .. }) => {
                    if lease.is_some_and(|id| !self.leases.contains_key(&id)) {
                        return Err(StoreError::LeaseNotFound);
                    }
                    if self
                        .entry(key)
                        .is_some_and(|entry| entry.version == i64::MAX)
                    {
                        return Err(StoreError::Backend);
                    }
                    changes = true;
                }
                RawTxnOp::Write(Mutation::Delete { key }) => changes |= self.entry(key).is_some(),
                _ => {}
            }
        }
        if changes {
            self.bump()?;
        }
        let mut responses = Vec::with_capacity(ops.len());
        for op in ops {
            let mut response = RawTxnResponseItem::default();
            match op {
                RawTxnOp::Get(key) => {
                    if let Some(entry) = self.entry(&key) {
                        response.key_value_pairs.push(KeyValuePair {
                            key,
                            value: entry.value.clone(),
                        });
                    }
                }
                RawTxnOp::GetRange { key, end, limit } => {
                    let request = raw_range::request(&key, &end, limit);
                    let mut entries = self.scan(&request, self.revision);
                    response.key_value_pairs = entries
                        .by_ref()
                        .take(if limit == 0 { usize::MAX } else { limit })
                        .map(|entry| KeyValuePair {
                            key: entry.key.clone(),
                            value: entry.value.clone(),
                        })
                        .collect();
                    response.more = entries.next().is_some();
                }
                RawTxnOp::Write(mutation) => self.write(mutation),
            }
            responses.push(response);
        }
        Ok(RawTxnResponse {
            succeeded,
            revision: self.revision,
            responses,
        })
    }

    fn bump(&mut self) -> Result<()> {
        self.revision = self.revision.checked_add(1).ok_or(StoreError::Backend)?;
        Ok(())
    }

    fn write(&mut self, mutation: Mutation) {
        let (key, entry) = match mutation {
            Mutation::Put { key, value, lease } => {
                let previous = self.entry(&key);
                let entry = Entry {
                    key: key.clone(),
                    value,
                    lease,
                    create_revision: previous.map_or(self.revision, |e| e.create_revision),
                    mod_revision: self.revision,
                    version: previous.map_or(1, |e| e.version + 1),
                };
                (key, Some(entry))
            }
            Mutation::Delete { key } => {
                if self.entry(&key).is_none() {
                    return;
                }
                (key, None)
            }
        };
        let previous = self.entry(&key).map(|e| e.value.clone());
        let event = entry.clone().map_or_else(
            || Event::Delete {
                key: key.clone(),
                revision: self.revision,
            },
            Event::Put,
        );
        self.keys.entry(key).or_default().push(Version {
            revision: self.revision,
            entry,
        });
        self.changes
            .entry(self.revision)
            .or_default()
            .push(Change { event, previous });
    }

    fn compact(&mut self, revision: Revision) -> Result<()> {
        if revision <= 0 {
            return Err(StoreError::InvalidArgument);
        }
        if revision <= self.compacted {
            return Err(StoreError::Compacted {
                requested: revision,
                compact_revision: Some(self.compacted),
            });
        }
        if revision > self.revision {
            return Err(StoreError::FutureRevision);
        }
        self.compacted = revision;
        self.changes = self.changes.split_off(&revision);
        self.keys.retain(|_, history| {
            let before = history.partition_point(|v| v.revision <= revision);
            history.drain(..before.saturating_sub(1));
            history.len() > 1 || history[0].entry.is_some()
        });
        Ok(())
    }

    pub(super) fn clock(&self) -> Option<u64> {
        (!self.leases.is_empty()).then_some(self.lease_clock)
    }

    pub(super) fn ttl(&self, id: LeaseId) -> Result<i64> {
        let lease = self.leases.get(&id).ok_or(StoreError::LeaseNotFound)?;
        Ok(lease
            .expires
            .saturating_sub(self.lease_clock)
            .div_ceil(1000) as i64)
    }

    fn deadline(&self, ttl: i64) -> Result<u64> {
        self.lease_clock
            .checked_add(ttl as u64 * 1000)
            .ok_or(StoreError::Backend)
    }

    fn grant(&mut self, ttl_seconds: i64) -> Result<Lease> {
        if !(1..=i32::MAX as i64).contains(&ttl_seconds) {
            return Err(StoreError::InvalidArgument);
        }
        let expires = self.deadline(ttl_seconds)?;
        let id = self.next_lease;
        self.next_lease = id.checked_add(1).ok_or(StoreError::Backend)?;
        self.leases.insert(
            id,
            LeaseDeadline {
                ttl_seconds,
                expires,
            },
        );
        Ok(Lease { id, ttl_seconds })
    }

    fn renew(&mut self, id: LeaseId) -> Result<Lease> {
        let ttl_seconds = self
            .leases
            .get(&id)
            .ok_or(StoreError::LeaseNotFound)?
            .ttl_seconds;
        let expires = self.deadline(ttl_seconds)?;
        self.leases
            .get_mut(&id)
            .ok_or(StoreError::LeaseNotFound)?
            .expires = expires;
        Ok(Lease { id, ttl_seconds })
    }

    fn lease_keys(&self, id: LeaseId) -> Vec<Vec<u8>> {
        self.keys
            .keys()
            .filter(|key| self.entry(key).is_some_and(|e| e.lease == Some(id)))
            .cloned()
            .collect()
    }

    fn remove_lease(&mut self, id: LeaseId) -> Result<()> {
        if !self.leases.contains_key(&id) {
            return Err(StoreError::LeaseNotFound);
        }
        let keys = self.lease_keys(id);
        if !keys.is_empty() {
            self.bump()?;
            for key in keys {
                self.write(Mutation::Delete { key });
            }
        }
        self.leases.remove(&id);
        Ok(())
    }

    fn advance_clock(&mut self, time: u64) -> Result<()> {
        if time <= self.lease_clock {
            return Ok(());
        }
        let expired: Vec<_> = self
            .leases
            .iter()
            .filter_map(|(id, lease)| (lease.expires <= time).then_some(*id))
            .collect();
        self.revision
            .checked_add(expired.len() as i64)
            .ok_or(StoreError::Backend)?;
        for id in expired {
            self.remove_lease(id)?;
        }
        self.lease_clock = time;
        Ok(())
    }
}

impl RecoveryImage {
    pub(super) fn validate(&self) -> Result<()> {
        if !(1..i64::MAX).contains(&self.revision)
            || self.next_lease <= 0
            || self
                .entries
                .windows(2)
                .any(|pair| pair[0].key >= pair[1].key)
            || self.entries.iter().any(|entry| {
                entry.key.is_empty()
                    || entry.create_revision <= 0
                    || entry.create_revision > entry.mod_revision
                    || entry.mod_revision > self.revision
                    || entry.version <= 0
                    || entry
                        .lease
                        .is_some_and(|id| id <= 0 || id >= self.next_lease)
            })
        {
            return Err(StoreError::InvalidArgument);
        }
        Ok(())
    }
}

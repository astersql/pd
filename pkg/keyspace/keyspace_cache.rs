// Copyright 2026 AsterSQL.
use crate::{BoundType, extract_keyspace_id, make_region_bound};
use astersql_pd_keyspace_constant::{MAX_VALID_KEYSPACE_ID, START_KEYSPACE_ID};
use std::{collections::BTreeMap, sync::RwLock};
/// State is independent of protobuf; unknown wire values remain visible unless tombstoned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyspaceState {
    Enabled,
    Disabled,
    Archived,
    Tombstone,
    Unknown(i32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyspaceItem {
    pub id: u32,
    pub name: String,
    pub state: KeyspaceState,
}
/// Shared ordered cache. Each read observes one mutation-complete snapshot.
#[derive(Default)]
pub struct Cache {
    tree: RwLock<BTreeMap<u32, KeyspaceItem>>,
}
pub trait Checker: Send + Sync {
    fn keyspace_exist(&self, id: u32) -> bool;
    fn ids_in_range(&self, start: u32, end: u32, limit: isize) -> (Vec<u32>, bool);
}
impl Cache {
    pub fn save(&self, id: u32, name: impl Into<String>, state: KeyspaceState) {
        let item = KeyspaceItem {
            id,
            name: name.into(),
            state,
        };
        self.tree.write().unwrap().insert(id, item);
    }
    pub fn delete_keyspace(&self, id: u32) {
        self.tree.write().unwrap().remove(&id);
    }
    pub fn get(&self, id: u32) -> Option<KeyspaceItem> {
        self.tree.read().unwrap().get(&id).cloned()
    }
    /// Ascending order includes tombstones; a false callback stops at that item.
    pub fn scan_all(&self, mut visit: impl FnMut(u32, &str) -> bool) {
        for item in self.tree.read().unwrap().values() {
            if !visit(item.id, &item.name) {
                break;
            }
        }
    }
    pub fn keyspace_exist(&self, id: u32) -> bool {
        self.tree
            .read()
            .unwrap()
            .get(&id)
            .is_some_and(|item| item.state != KeyspaceState::Tombstone)
    }
    /// Inclusive descending range; zero/negative limits are unbounded.
    pub fn ids_in_range(&self, start: u32, end: u32, limit: isize) -> (Vec<u32>, bool) {
        if start > end {
            return (vec![], false);
        }
        let tree = self.tree.read().unwrap();
        let mut ids = vec![];
        for (id, item) in tree.range(start..=end).rev() {
            if item.state == KeyspaceState::Tombstone {
                continue;
            }
            ids.push(*id);
            if limit > 0 && ids.len() >= limit as usize {
                break;
            }
        }
        let found = !ids.is_empty();
        (ids, found)
    }
}
impl Checker for Cache {
    fn keyspace_exist(&self, id: u32) -> bool {
        self.keyspace_exist(id)
    }
    fn ids_in_range(&self, start: u32, end: u32, limit: isize) -> (Vec<u32>, bool) {
        self.ids_in_range(start, end, limit)
    }
}
/// Conservative boundary check, retaining Go's ordered early-return cases.
pub fn region_spans_multiple_keyspaces(
    start: &[u8],
    end: &[u8],
    checker: Option<&dyn Checker>,
) -> bool {
    let Some(checker) = checker else {
        return false;
    };
    let start_info = if start.is_empty() {
        Some((START_KEYSPACE_ID, BoundType::Raw))
    } else {
        extract_keyspace_id(start)
    };
    let end_info = extract_keyspace_id(end);
    if start_info.is_none() && (end_info.is_none() || end.is_empty()) {
        return false;
    }
    let (Some((start_id, start_mode)), Some((end_id, end_mode))) = (start_info, end_info) else {
        return true;
    };
    if start_id == end_id && start_mode == end_mode {
        return false;
    }
    if start_mode == BoundType::Raw && end_mode == BoundType::Txn {
        return true;
    }
    if end.is_empty() {
        return checker.keyspace_exist(start_id);
    }
    if end_id == start_id + 1 {
        let b = make_region_bound(start_id);
        if end == b.txn_right_bound || end == b.raw_right_bound {
            return false;
        }
    }
    checker.keyspace_exist(start_id) || checker.keyspace_exist(end_id)
}
/// At most ten highest live IDs supply strict internal split boundaries, sorted and deduplicated.
pub fn get_keyspace_split_keys(
    start: &[u8],
    end: &[u8],
    key_type: &str,
    checker: Option<&dyn Checker>,
) -> Vec<Vec<u8>> {
    let Some(checker) = checker else {
        return vec![];
    };
    let bound = BoundType::from_key_type(key_type);
    let start_id = if start.is_empty() {
        START_KEYSPACE_ID
    } else {
        extract_keyspace_id(start)
            .filter(|(_, mode)| *mode == bound)
            .map(|(id, _)| id)
            .unwrap_or(START_KEYSPACE_ID)
    };
    let end_id = if end.is_empty() {
        MAX_VALID_KEYSPACE_ID
    } else {
        extract_keyspace_id(end)
            .filter(|(_, mode)| *mode == bound)
            .map(|(id, _)| id)
            .unwrap_or(MAX_VALID_KEYSPACE_ID)
    };
    if start_id >= end_id {
        return vec![];
    }
    let (ids, found) = checker.ids_in_range(start_id, end_id, 10);
    if !found || ids.is_empty() {
        return vec![];
    }
    let mut keys = vec![];
    for id in ids {
        let b = make_region_bound(id);
        let (lo, hi) = bound.bounds(&b);
        for key in [lo, hi] {
            if astersql_pd_keyutil::between(start, end, key) {
                keys.push(key.to_vec());
            }
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

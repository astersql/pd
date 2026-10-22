// Copyright 2026 AsterSQL.
//! Region lookup over the canonical B-tree, ordered solely by start key.
use crate::{KeyRange, RegionInfo, RegionMeta};
use astersql_pd_btree::BTree;
use std::sync::Arc;

/// An indexed region. Shared ownership preserves lookup identity.
#[derive(Clone, Debug)]
pub struct RegionItem {
    region: Arc<RegionInfo>,
}
impl RegionItem {
    /// Wraps an existing region without copying its payload.
    pub fn new(region: Arc<RegionInfo>) -> Self {
        Self { region }
    }
    /// Returns the indexed region.
    pub fn region(&self) -> &Arc<RegionInfo> {
        &self.region
    }
    /// Compares start keys only; IDs and end keys do not affect ordering.
    pub fn less(&self, other: &Self) -> bool {
        self.start_key() < other.start_key()
    }
    fn start_key(&self) -> &[u8] {
        &self.region.meta().key_range.start_key
    }
    fn pivot(key: &[u8]) -> Self {
        Self::new(Arc::new(RegionInfo::new(
            RegionMeta {
                key_range: KeyRange {
                    start_key: key.to_vec(),
                    end_key: vec![],
                },
                ..Default::default()
            },
            None,
            vec![],
            vec![],
        )))
    }
}
type RegionIndex = BTree<RegionItem, fn(&RegionItem, &RegionItem) -> bool>;
/// Readable region index. Overlap-aware mutations are implemented separately.
pub struct RegionTree {
    pub(crate) tree: RegionIndex,
}
impl Default for RegionTree {
    fn default() -> Self {
        Self {
            tree: BTree::new(64, RegionItem::less),
        }
    }
}
impl RegionTree {
    /// Creates an empty region index.
    pub fn new() -> Self {
        Self::default()
    }
    /// Builds an index from non-overlapping regions, replacing equal start keys.
    /// Callers must supply disjoint ranges; this constructor does not resolve overlaps.
    pub fn from_regions(regions: impl IntoIterator<Item = Arc<RegionInfo>>) -> Self {
        let mut result = Self::new();
        for region in regions {
            result.tree.replace_or_insert(RegionItem::new(region));
        }
        result
    }
    /// Finds the region containing `key` in its half-open range.
    pub fn search(&self, key: &[u8]) -> Option<Arc<RegionInfo>> {
        let pivot = RegionItem::pivot(key);
        let (_, rank) = self.tree.get_with_index(&pivot);
        let index = if self.tree.has(&pivot) {
            rank
        } else {
            rank.checked_sub(1)?
        };
        let region = &self.tree.get_at(index as isize)?.region;
        region
            .meta()
            .key_range
            .contains(key)
            .then(|| region.clone())
    }
    /// Returns the contiguous predecessor of the region containing `key`.
    pub fn search_prev(&self, key: &[u8]) -> Option<Arc<RegionInfo>> {
        let current = self.search(key)?;
        let (previous, _) = self.get_adjacent_regions(&current);
        previous.filter(|p| p.meta().key_range.end_key == current.meta().key_range.start_key)
    }
    /// Looks up keys in input order, retaining duplicates and misses.
    pub fn search_by_keys(&self, keys: &[&[u8]]) -> Vec<Option<Arc<RegionInfo>>> {
        keys.iter().map(|key| self.search(key)).collect()
    }
    /// Looks up contiguous predecessors in input order, retaining misses.
    pub fn search_by_prev_keys(&self, keys: &[&[u8]]) -> Vec<Option<Arc<RegionInfo>>> {
        keys.iter().map(|key| self.search_prev(key)).collect()
    }
    /// Returns strict neighbors by start key, whether or not the pivot is indexed.
    /// Neighbor ranges need not be contiguous with the pivot.
    pub fn get_adjacent_regions(
        &self,
        region: &RegionInfo,
    ) -> (Option<Arc<RegionInfo>>, Option<Arc<RegionInfo>>) {
        let pivot = RegionItem::pivot(&region.meta().key_range.start_key);
        let (exact, rank) = self.tree.get_with_index(&pivot);
        let prev = rank
            .checked_sub(1)
            .and_then(|i| self.tree.get_at(i as isize))
            .map(|i| i.region.clone());
        let next = self
            .tree
            .get_at((rank + usize::from(exact.is_some())) as isize)
            .map(|i| i.region.clone());
        (prev, next)
    }
}

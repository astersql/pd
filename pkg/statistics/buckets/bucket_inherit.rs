// Copyright 2026 AsterSQL.

use astersql_pd_keyutil::{max_key, min_key};

use crate::BucketTreeItem;

impl BucketTreeItem {
    /// Inherits only heat from ordered, disjoint origin stats, including archives.
    ///
    /// Positive degrees dominate cold degrees; otherwise the coldest degree wins.
    /// Origins must be ordered by start key, as returned by `BucketRangeIndex`.
    /// Preserves the source's ordinary byte comparisons for empty end keys,
    /// including its early return when this item's end precedes the first origin.
    pub fn inherit(&mut self, origins: &[BucketTreeItem]) {
        if origins.is_empty() || self.stats.is_empty() || self.end_key < origins[0].start_key {
            return;
        }
        let mut old = origins.iter().flat_map(|item| &item.stats).peekable();
        let mut cursor = 0;
        while cursor < self.stats.len() {
            let Some(origin) = old.peek() else {
                break;
            };
            let new = &mut self.stats[cursor];
            let left = max_key(&new.start_key, &origin.start_key);
            let right = min_key(&new.end_key, &origin.end_key);
            if left < right || right.is_empty() {
                let previous = origin.hot_degree;
                let current = new.hot_degree;
                if (previous < 0 && current <= 0 && previous < current)
                    || (previous > 0 && previous > current)
                {
                    new.hot_degree = previous;
                }
            }
            if new.end_key > origin.end_key || new.end_key.is_empty() {
                old.next();
            } else {
                cursor += 1;
            }
        }
    }
}

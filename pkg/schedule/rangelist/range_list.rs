// Copyright 2026 AsterSQL.
use crate::List;
impl<T> List<T> {
    fn first_after(&self, key: &[u8]) -> usize {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.get(mid).0 <= key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
    fn data_before(&self, i: usize) -> (isize, Option<&[T]>) {
        i.checked_sub(1).map_or((-1, None), |index| {
            (index as isize, Some(self.get(index).1))
        })
    }
    fn crosses_next(&self, i: usize, end: &[u8]) -> bool {
        i < self.len() && (end.is_empty() || self.get(i).0 < end)
    }
    /// Returns the segment index and data containing the key, or (-1, None).
    pub fn get_data_by_key(&self, key: &[u8]) -> (isize, Option<&[T]>) {
        self.data_before(self.first_after(key))
    }
    /// Returns a single segment containing the entire range, or (-1, None).
    /// Empty end means infinity. An end exactly on a boundary is allowed.
    pub fn get_data(&self, start: &[u8], end: &[u8]) -> (isize, Option<&[T]>) {
        let i = self.first_after(start);
        if self.crosses_next(i, end) {
            (-1, None)
        } else {
            self.data_before(i)
        }
    }
    /// Returns split keys strictly inside the range; empty end means infinity.
    pub fn get_split_keys(&self, start: &[u8], end: &[u8]) -> Vec<&[u8]> {
        (self.first_after(start)..self.len())
            .map(|i| self.get(i).0)
            .take_while(|key| end.is_empty() || *key < end)
            .collect()
    }
    /// Returns start data and whether the range has no interior split point.
    /// A range before the first segment is covered with absent data.
    pub fn get_data_by_range(&self, start: &[u8], end: &[u8]) -> (Option<&[T]>, bool) {
        let i = self.first_after(start);
        if self.crosses_next(i, end) {
            (None, false)
        } else {
            (self.data_before(i).1, true)
        }
    }
}

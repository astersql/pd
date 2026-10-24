// Copyright 2026 AsterSQL.
use std::cmp::Ordering;
type Compare<T> = Box<dyn Fn(&T, &T) -> Ordering>;
/// Builds disjoint segments from overlapping ranges.
pub struct Builder<T> {
    points: Vec<(Vec<u8>, bool, T)>,
    compare: Option<Compare<T>>,
}
/// Ordered key segments and their active items.
#[derive(Debug, PartialEq, Eq)]
pub struct List<T> {
    segments: Vec<(Vec<u8>, Vec<T>)>,
}
impl<T> Default for Builder<T> {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            compare: None,
        }
    }
}
impl<T: Clone + PartialEq> Builder<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_compare_func(&mut self, compare: impl Fn(&T, &T) -> Ordering + 'static) {
        self.compare = Some(Box::new(compare));
    }
    pub fn add_item(&mut self, start: &[u8], end: &[u8], data: T) {
        self.points.push((start.to_vec(), true, data.clone()));
        if !end.is_empty() {
            self.points.push((end.to_vec(), false, data));
        }
    }
    pub fn build(&mut self) -> List<T> {
        crate::event_sort::sort(&mut self.points);
        let mut items = Vec::new();
        let mut segments = Vec::new();
        for (i, (key, start, data)) in self.points.iter().enumerate() {
            if *start {
                let pos = self.compare.as_ref().map_or(items.len(), |compare| {
                    items.partition_point(|item| compare(item, data) != Ordering::Greater)
                });
                items.insert(pos, data.clone());
            } else if let Some(pos) = items.iter().position(|item| item == data) {
                items.remove(pos);
            }
            if self.points.get(i + 1).is_none_or(|next| next.0 != *key) {
                segments.push((key.clone(), items.clone()));
            }
        }
        List { segments }
    }
}
impl<T> List<T> {
    pub fn len(&self) -> usize {
        self.segments.len()
    }
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
    pub fn get(&self, index: usize) -> (&[u8], &[T]) {
        let (key, data) = &self.segments[index];
        (key, data)
    }
}

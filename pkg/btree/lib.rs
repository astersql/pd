// Copyright 2026 AsterSQL.
//! In-memory B-tree with caller-defined strict weak ordering.
//! Node layout, insertion, deletion and ordered traversal follow btree_generic.go.
//! Copy-on-write and free-list APIs are introduced in later ports.

struct Node<T> {
    items: Vec<T>,
    children: Vec<Node<T>>,
}

impl<T> Node<T> {
    fn empty() -> Self {
        Self {
            items: Vec::new(),
            children: Vec::new(),
        }
    }

    fn find(&self, key: &T, less: &impl Fn(&T, &T) -> bool) -> (usize, bool) {
        let index = self.items.partition_point(|item| less(item, key));
        (
            index,
            index < self.items.len() && !less(key, &self.items[index]),
        )
    }

    // Seek the inclusive lower bound in each node, then propagate callback or
    // exclusive-upper-bound termination through every ancestor.
    fn ascend(
        &self,
        start: Option<&T>,
        stop: Option<&T>,
        less: &impl Fn(&T, &T) -> bool,
        visit: &mut impl FnMut(&T) -> bool,
    ) -> bool {
        let index = start.map_or(0, |key| self.find(key, less).0);
        for i in index..self.items.len() {
            if let Some(child) = self.children.get(i)
                && !child.ascend(start, stop, less, visit)
            {
                return false;
            }
            let item = &self.items[i];
            if stop.is_some_and(|bound| !less(item, bound)) || !visit(item) {
                return false;
            }
        }
        self.children
            .last()
            .is_none_or(|child| child.ascend(start, stop, less, visit))
    }

    // Seek the inclusive upper bound and propagate stop through every ancestor.
    fn descend(
        &self,
        start: Option<&T>,
        stop: Option<&T>,
        less: &impl Fn(&T, &T) -> bool,
        visit: &mut impl FnMut(&T) -> bool,
    ) -> bool {
        let end = start.map_or(self.items.len(), |key| {
            self.items.partition_point(|item| !less(key, item))
        });
        for i in (0..end).rev() {
            if let Some(child) = self.children.get(i + 1)
                && !child.descend(start, stop, less, visit)
            {
                return false;
            }
            let item = &self.items[i];
            if stop.is_some_and(|bound| !less(bound, item)) || !visit(item) {
                return false;
            }
        }
        self.children
            .first()
            .is_none_or(|child| child.descend(start, stop, less, visit))
    }

    fn split_child(&mut self, index: usize, degree: usize) {
        let child = &mut self.children[index];
        let right_items = child.items.split_off(degree);
        let median = child.items.pop().expect("full child has a median");
        let right_children = if child.children.is_empty() {
            Vec::new()
        } else {
            child.children.split_off(degree)
        };
        self.items.insert(index, median);
        self.children.insert(
            index + 1,
            Node {
                items: right_items,
                children: right_children,
            },
        );
    }

    // Grow a minimal child before descending, preferring the left sibling just
    // as Go growChildAndRemove does. Moving values avoids requiring T: Clone.
    fn grow_child(&mut self, index: usize, degree: usize) {
        if index > 0 && self.children[index - 1].items.len() >= degree {
            let (left, right) = self.children.split_at_mut(index);
            let sibling = &mut left[index - 1];
            let child = &mut right[0];
            let stolen = sibling.items.pop().unwrap();
            child
                .items
                .insert(0, std::mem::replace(&mut self.items[index - 1], stolen));
            if let Some(branch) = sibling.children.pop() {
                child.children.insert(0, branch);
            }
        } else if index < self.items.len() && self.children[index + 1].items.len() >= degree {
            let (left, right) = self.children.split_at_mut(index + 1);
            let child = &mut left[index];
            let sibling = &mut right[0];
            let stolen = sibling.items.remove(0);
            child
                .items
                .push(std::mem::replace(&mut self.items[index], stolen));
            if !sibling.children.is_empty() {
                child.children.push(sibling.children.remove(0));
            }
        } else {
            let index = index.min(self.items.len() - 1);
            let separator = self.items.remove(index);
            let right = self.children.remove(index + 1);
            let left = &mut self.children[index];
            left.items.push(separator);
            left.items.extend(right.items);
            left.children.extend(right.children);
        }
    }

    fn remove_min(&mut self, degree: usize) -> T {
        if self.children.is_empty() {
            return self.items.remove(0);
        }
        if self.children[0].items.len() < degree {
            self.grow_child(0, degree);
            return self.remove_min(degree);
        }
        self.children[0].remove_min(degree)
    }

    fn remove_max(&mut self, degree: usize) -> T {
        if self.children.is_empty() {
            return self.items.pop().unwrap();
        }
        let index = self.items.len();
        if self.children[index].items.len() < degree {
            self.grow_child(index, degree);
            return self.remove_max(degree);
        }
        self.children[index].remove_max(degree)
    }

    fn remove(&mut self, key: &T, degree: usize, less: &impl Fn(&T, &T) -> bool) -> Option<T> {
        let (index, found) = self.find(key, less);
        if self.children.is_empty() {
            return found.then(|| self.items.remove(index));
        }
        if self.children[index].items.len() < degree {
            self.grow_child(index, degree);
            // Rotations and merges may move the requested item; search again.
            return self.remove(key, degree, less);
        }
        if found {
            let predecessor = self.children[index].remove_max(degree);
            Some(std::mem::replace(&mut self.items[index], predecessor))
        } else {
            self.children[index].remove(key, degree, less)
        }
    }

    fn insert(&mut self, item: T, degree: usize, less: &impl Fn(&T, &T) -> bool) -> Option<T> {
        let (mut index, found) = self.find(&item, less);
        if found {
            return Some(std::mem::replace(&mut self.items[index], item));
        }
        if self.children.is_empty() {
            self.items.insert(index, item);
            return None;
        }
        if self.children[index].items.len() == 2 * degree - 1 {
            self.split_child(index, degree);
            if less(&self.items[index], &item) {
                index += 1;
            } else if !less(&item, &self.items[index]) {
                return Some(std::mem::replace(&mut self.items[index], item));
            }
        }
        self.children[index].insert(item, degree, less)
    }
}

/// An ordered tree storing one item per equivalence class of `less`.
/// The comparator must define a strict weak ordering and remain stable.
pub struct BTree<T, F> {
    degree: usize,
    root: Option<Node<T>>,
    length: usize,
    less: F,
}

impl<T, F: Fn(&T, &T) -> bool> BTree<T, F> {
    /// Creates a tree; panics with `bad degree` for degrees below two or
    /// degrees whose maximum node size cannot be represented.
    pub fn new(degree: usize, less: F) -> Self {
        assert!(degree > 1 && degree <= usize::MAX / 2, "bad degree");
        Self {
            degree,
            root: None,
            length: 0,
            less,
        }
    }

    /// Inserts an item, returning the previous equivalent item when replaced.
    pub fn replace_or_insert(&mut self, item: T) -> Option<T> {
        if self
            .root
            .as_ref()
            .is_some_and(|root| root.items.len() == 2 * self.degree - 1)
        {
            let old = self.root.take().unwrap();
            let mut root = Node::empty();
            root.children.push(old);
            root.split_child(0, self.degree);
            self.root = Some(root);
        }
        let previous =
            self.root
                .get_or_insert_with(Node::empty)
                .insert(item, self.degree, &self.less);
        if previous.is_none() {
            self.length += 1;
        }
        previous
    }

    /// Removes and returns the stored item equivalent to `key`, if present.
    pub fn delete(&mut self, key: &T) -> Option<T> {
        let removed = self.root.as_mut()?.remove(key, self.degree, &self.less);
        if removed.is_some() {
            self.length -= 1;
        }
        if self.root.as_ref().is_some_and(|root| root.items.is_empty()) {
            let mut root = self.root.take().unwrap();
            self.root = root.children.pop();
        }
        removed
    }

    /// Removes the first item in comparator order, or returns `None` when empty.
    pub fn delete_min(&mut self) -> Option<T> {
        let removed = self.root.as_mut()?.remove_min(self.degree);
        self.finish_extreme_removal();
        Some(removed)
    }

    /// Removes the last item in comparator order, or returns `None` when empty.
    pub fn delete_max(&mut self) -> Option<T> {
        let removed = self.root.as_mut()?.remove_max(self.degree);
        self.finish_extreme_removal();
        Some(removed)
    }

    fn finish_extreme_removal(&mut self) {
        self.length -= 1;
        if self.root.as_ref().is_some_and(|root| root.items.is_empty()) {
            let mut root = self.root.take().unwrap();
            self.root = root.children.pop();
        }
    }

    /// Visits all items in comparator order, stopping when `visit` returns false.
    pub fn ascend(&self, mut visit: impl FnMut(&T) -> bool) {
        self.ascend_bounds(None, None, &mut visit);
    }

    /// Visits items in comparator order within `[greater_or_equal, less_than)`.
    /// Returning false from `visit` stops traversal immediately.
    pub fn ascend_range(
        &self,
        greater_or_equal: &T,
        less_than: &T,
        mut visit: impl FnMut(&T) -> bool,
    ) {
        self.ascend_bounds(Some(greater_or_equal), Some(less_than), &mut visit);
    }

    /// Visits items strictly less than `pivot` according to the comparator.
    /// Returning false from `visit` stops traversal immediately.
    pub fn ascend_less_than(&self, pivot: &T, mut visit: impl FnMut(&T) -> bool) {
        self.ascend_bounds(None, Some(pivot), &mut visit);
    }

    /// Visits items greater than or equivalent to `pivot` in comparator order.
    /// Returning false from `visit` stops traversal immediately.
    pub fn ascend_greater_or_equal(&self, pivot: &T, mut visit: impl FnMut(&T) -> bool) {
        self.ascend_bounds(Some(pivot), None, &mut visit);
    }

    fn ascend_bounds(
        &self,
        start: Option<&T>,
        stop: Option<&T>,
        visit: &mut impl FnMut(&T) -> bool,
    ) {
        if let Some(root) = &self.root {
            root.ascend(start, stop, &self.less, visit);
        }
    }

    /// Visits all items in reverse comparator order until `visit` returns false.
    pub fn descend(&self, mut visit: impl FnMut(&T) -> bool) {
        self.descend_bounds(None, None, &mut visit);
    }

    /// Visits items `greater_than < item <= less_or_equal` in reverse comparator
    /// order. Returning false from `visit` stops traversal immediately.
    pub fn descend_range(
        &self,
        less_or_equal: &T,
        greater_than: &T,
        mut visit: impl FnMut(&T) -> bool,
    ) {
        self.descend_bounds(Some(less_or_equal), Some(greater_than), &mut visit);
    }

    /// Visits items less than or equivalent to `pivot` in reverse comparator
    /// order, stopping when `visit` returns false.
    pub fn descend_less_or_equal(&self, pivot: &T, mut visit: impl FnMut(&T) -> bool) {
        self.descend_bounds(Some(pivot), None, &mut visit);
    }

    /// Visits items strictly greater than `pivot` in reverse comparator order,
    /// stopping when `visit` returns false.
    pub fn descend_greater_than(&self, pivot: &T, mut visit: impl FnMut(&T) -> bool) {
        self.descend_bounds(None, Some(pivot), &mut visit);
    }

    fn descend_bounds(
        &self,
        start: Option<&T>,
        stop: Option<&T>,
        visit: &mut impl FnMut(&T) -> bool,
    ) {
        if let Some(root) = &self.root {
            root.descend(start, stop, &self.less, visit);
        }
    }

    /// Returns the stored item equivalent to `key`.
    pub fn get(&self, key: &T) -> Option<&T> {
        let mut current = self.root.as_ref()?;
        loop {
            let (index, found) = current.find(key, &self.less);
            if found {
                return Some(&current.items[index]);
            }
            current = current.children.get(index)?;
        }
    }

    /// Returns the first item in comparator order, or `None` for an empty tree.
    pub fn min(&self) -> Option<&T> {
        let mut node = self.root.as_ref()?;
        while let Some(child) = node.children.first() {
            node = child;
        }
        node.items.first()
    }

    /// Returns the last item in comparator order, or `None` for an empty tree.
    pub fn max(&self) -> Option<&T> {
        let mut node = self.root.as_ref()?;
        while let Some(child) = node.children.last() {
            node = child;
        }
        node.items.last()
    }

    /// Reports whether an equivalent item exists.
    pub fn has(&self, key: &T) -> bool {
        self.get(key).is_some()
    }

    /// Returns the number of stored items.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Reports whether the tree contains no items.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

#[cfg(test)]
#[path = "btree_test.rs"]
mod tests;

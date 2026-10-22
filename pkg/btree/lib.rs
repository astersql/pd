// Copyright 2026 AsterSQL.
//! In-memory B-tree with caller-defined strict weak ordering.
//! Node layout and top-down insertion follow btree_generic.go. Range traversal,
//! deletion, copy-on-write and free-list APIs are introduced in later ports.

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

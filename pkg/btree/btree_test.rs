// Copyright 2026 AsterSQL.
use super::*;

#[test]
fn insert_get_and_len() {
    for degree in [2, 3, 8, 32] {
        let mut tree = BTree::new(degree, |a: &(i32, i32), b: &(i32, i32)| a.0 < b.0);
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
        assert_eq!(tree.get(&(10, 0)), None);
        for i in (0..1000).map(|i| (i * 317) % 1000) {
            assert_eq!(tree.replace_or_insert((i, i)), None);
            assert!(tree.has(&(i, 0)));
        }
        assert_eq!(tree.len(), 1000);
        for i in 0..1000 {
            assert_eq!(tree.replace_or_insert((i, -i)), Some((i, i)));
            assert_eq!(tree.get(&(i, 99)), Some(&(i, -i)));
        }
        assert_eq!(tree.len(), 1000);
        assert!(!tree.has(&(1000, 0)));
        let mut ordered = Vec::new();
        collect(tree.root.as_ref().unwrap(), &mut ordered);
        assert_eq!(ordered, (0..1000).map(|i| (i, -i)).collect::<Vec<_>>());
    }
}

fn collect(node: &Node<(i32, i32)>, out: &mut Vec<(i32, i32)>) {
    for (i, item) in node.items.iter().enumerate() {
        if !node.children.is_empty() {
            collect(&node.children[i], out);
        }
        out.push(*item);
    }
    if let Some(last) = node.children.last() {
        collect(last, out);
    }
}

#[test]
#[should_panic(expected = "bad degree")]
fn invalid_degree() {
    let _ = BTree::new(1, |a: &i32, b: &i32| a < b);
}

#[test]
fn descending_order() {
    let mut tree = BTree::new(2, |a: &i32, b: &i32| a > b);
    for i in 0..1000 {
        assert_eq!(tree.replace_or_insert(i), None);
    }
    for i in 0..1000 {
        assert_eq!(tree.get(&i), Some(&i));
    }
    fn visit(node: &Node<i32>, values: &mut Vec<i32>, degree: usize) -> usize {
        assert!(node.items.len() < 2 * degree);
        assert!(node.items.windows(2).all(|pair| pair[0] > pair[1]));
        if node.children.is_empty() {
            values.extend(&node.items);
            return 0;
        }
        assert_eq!(node.children.len(), node.items.len() + 1);
        let mut heights = Vec::new();
        for (i, child) in node.children.iter().enumerate() {
            assert!(child.items.len() >= degree - 1);
            heights.push(visit(child, values, degree));
            if let Some(item) = node.items.get(i) {
                values.push(*item);
            }
        }
        assert!(heights.iter().all(|height| *height == heights[0]));
        heights[0] + 1
    }
    let mut values = Vec::new();
    assert!(visit(tree.root.as_ref().unwrap(), &mut values, 2) > 1);
    assert_eq!(values, (0..1000).rev().collect::<Vec<_>>());
}

#[test]
#[should_panic(expected = "bad degree")]
fn zero_degree() {
    let _ = BTree::new(0, |a: &i32, b: &i32| a < b);
}

#[test]
fn split_replace_min_max() {
    for degree in [2, 3, 8] {
        for descending in [false, true] {
            let mut tree = BTree::new(
                degree,
                |a: &(i32, i32), b: &(i32, i32)| {
                    if descending { a.0 > b.0 } else { a.0 < b.0 }
                },
            );
            assert_eq!(tree.min(), None);
            assert_eq!(tree.max(), None);
            let mut expected = std::collections::BTreeMap::new();
            for key in (0..2048).map(|i| (i * 317) % 2048) {
                assert_eq!(tree.replace_or_insert((key, key)), None);
                expected.insert(key, key);
                let first = *expected.first_key_value().unwrap().0;
                let last = *expected.last_key_value().unwrap().0;
                let (min, max) = if descending {
                    (last, first)
                } else {
                    (first, last)
                };
                assert_eq!(tree.min(), Some(&(min, min)));
                assert_eq!(tree.max(), Some(&(max, max)));
                assert_eq!(tree.len(), expected.len());
            }
            let mut node = tree.root.as_ref().unwrap();
            let mut height = 0;
            while let Some(child) = node.children.first() {
                height += 1;
                node = child;
            }
            assert!(height >= 2);
            for key in 0..2048 {
                assert_eq!(tree.replace_or_insert((key, -key - 1)), Some((key, key)));
                assert_eq!(tree.get(&(key, 0)), Some(&(key, -key - 1)));
                assert_eq!(tree.len(), 2048);
                let (min, max) = if descending { (2047, 0) } else { (0, 2047) };
                let value = |k: i32| if k <= key { -k - 1 } else { k };
                assert_eq!(tree.min(), Some(&(min, value(min))));
                assert_eq!(tree.max(), Some(&(max, value(max))));
            }
            let mut ordered = Vec::new();
            collect(tree.root.as_ref().unwrap(), &mut ordered);
            let mut expected: Vec<_> = (0..2048).map(|k| (k, -k - 1)).collect();
            if descending {
                expected.reverse();
            }
            assert_eq!(ordered, expected);
        }
    }
}

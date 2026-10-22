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

#[test]
fn delete_and_rebalance() {
    fn validate(
        node: &Node<(i32, i32)>,
        degree: usize,
        root: bool,
        out: &mut Vec<(i32, i32)>,
    ) -> usize {
        assert!(node.items.len() < 2 * degree);
        assert!(node.items.len() >= if root { 1 } else { degree - 1 });
        if node.children.is_empty() {
            out.extend(&node.items);
            return 0;
        }
        assert_eq!(node.children.len(), node.items.len() + 1);
        let mut height = None;
        for (i, child) in node.children.iter().enumerate() {
            let h = validate(child, degree, false, out);
            assert_eq!(*height.get_or_insert(h), h);
            if let Some(item) = node.items.get(i) {
                out.push(*item);
            }
        }
        height.unwrap() + 1
    }
    for degree in [2, 3, 8, 32] {
        for descending in [false, true] {
            for order in [0, 1, 2] {
                let mut tree = BTree::new(
                    degree,
                    |a: &(i32, i32), b: &(i32, i32)| {
                        if descending { a.0 > b.0 } else { a.0 < b.0 }
                    },
                );
                assert_eq!(tree.delete(&(0, 99)), None);
                let mut expected = std::collections::BTreeMap::new();
                for key in (0..512).map(|i| (i * 317) % 512) {
                    tree.replace_or_insert((key, -key - 1));
                    expected.insert(key, -key - 1);
                }
                for i in 0..512 {
                    let key = match order {
                        0 => i,
                        1 => 511 - i,
                        _ => (i * 317) % 512,
                    };
                    assert_eq!(tree.delete(&(key, 99)), Some((key, -key - 1)));
                    expected.remove(&key);
                    assert_eq!(tree.delete(&(key, 99)), None);
                    assert_eq!(tree.delete(&(1024, 0)), None);
                    assert_eq!(tree.len(), expected.len());
                    assert_eq!(tree.is_empty(), expected.is_empty());
                    assert_eq!(tree.get(&(key, 0)), None);
                    let mut values = Vec::new();
                    if let Some(root) = &tree.root {
                        validate(root, degree, true, &mut values);
                    }
                    let mut want: Vec<_> = expected.iter().map(|(&k, &v)| (k, v)).collect();
                    if descending {
                        want.reverse();
                    }
                    assert_eq!(values, want);
                    assert_eq!(tree.min(), want.first());
                    assert_eq!(tree.max(), want.last());
                    for item in &want {
                        assert_eq!(tree.get(&(item.0, 0)), Some(item));
                    }
                }
                assert!(tree.root.is_none());
                assert_eq!(tree.replace_or_insert((42, 7)), None);
                assert_eq!(tree.delete(&(42, 99)), Some((42, 7)));
                assert!(tree.is_empty());
            }
        }
    }
}

#[test]
fn delete_min_max() {
    for degree in [2, 3, 8, 32] {
        for descending in [false, true] {
            for mode in 0..3 {
                let mut tree = BTree::new(
                    degree,
                    |a: &(i32, String), b: &(i32, String)| {
                        if descending { a.0 > b.0 } else { a.0 < b.0 }
                    },
                );
                assert_eq!(tree.delete_min(), None);
                assert_eq!(tree.delete_max(), None);
                for key in (0..512).map(|i| (i * 317) % 512) {
                    tree.replace_or_insert((key, format!("value-{key}")));
                }
                let mut expected: std::collections::VecDeque<_> =
                    (0..512).map(|key| (key, format!("value-{key}"))).collect();
                if descending {
                    expected.make_contiguous().reverse();
                }
                for step in 0..512 {
                    let take_min = mode == 0 || (mode == 2 && step % 2 == 0);
                    let want = if take_min {
                        expected.pop_front()
                    } else {
                        expected.pop_back()
                    };
                    let got = if take_min {
                        tree.delete_min()
                    } else {
                        tree.delete_max()
                    };
                    assert_eq!(got, want);
                    assert_eq!(tree.len(), expected.len());
                    assert_eq!(tree.is_empty(), expected.is_empty());
                    assert_eq!(tree.min(), expected.front());
                    assert_eq!(tree.max(), expected.back());
                    for item in &expected {
                        assert_eq!(tree.get(&(item.0, String::new())), Some(item));
                    }
                }
                assert!(tree.root.is_none());
                assert_eq!(tree.delete_min(), None);
                assert_eq!(tree.delete_max(), None);
                tree.replace_or_insert((0, "zero".to_owned()));
                assert_eq!(tree.delete_min(), Some((0, "zero".to_owned())));
                tree.replace_or_insert((0, "zero".to_owned()));
                assert_eq!(tree.delete_max(), Some((0, "zero".to_owned())));
                assert!(tree.is_empty());
            }
        }
    }
}

#[test]
fn ascending_iteration() {
    for degree in [2, 3, 8] {
        for descending in [false, true] {
            let less = |a: &(i32, String), b: &(i32, String)| {
                if descending { a.0 > b.0 } else { a.0 < b.0 }
            };
            let mut tree = BTree::new(degree, less);
            tree.ascend(|_| panic!("empty tree visited"));
            tree.ascend_range(&(0, String::new()), &(10, String::new()), |_| {
                panic!("empty range visited")
            });
            tree.ascend_less_than(&(0, String::new()), |_| panic!("empty tree visited"));
            tree.ascend_greater_or_equal(&(0, String::new()), |_| panic!("empty tree visited"));
            for key in (0..128).map(|i| ((i * 37) % 128) * 2) {
                tree.replace_or_insert((key, format!("value-{key}")));
            }
            let mut ordered: Vec<_> = (0..128)
                .map(|i| (i * 2, format!("value-{}", i * 2)))
                .collect();
            if descending {
                ordered.reverse();
            }
            for lower in [-1, 0, 1, 62, 63, 64, 253, 254, 255] {
                for upper in [-1, 0, 1, 62, 63, 64, 253, 254, 255] {
                    let lower = (lower, String::new());
                    let upper = (upper, String::new());
                    for mode in 0..4 {
                        let want: Vec<_> = ordered
                            .iter()
                            .filter(|item| {
                                (mode == 0 || mode == 2 || !less(item, &lower))
                                    && (mode == 0 || mode == 3 || less(item, &upper))
                            })
                            .cloned()
                            .collect();
                        for limit in [1, 3, 17, 256] {
                            let mut got = Vec::new();
                            let mut visit = |item: &(i32, String)| {
                                got.push(item.clone());
                                got.len() < limit
                            };
                            match mode {
                                0 => tree.ascend(&mut visit),
                                1 => tree.ascend_range(&lower, &upper, &mut visit),
                                2 => tree.ascend_less_than(&upper, &mut visit),
                                _ => tree.ascend_greater_or_equal(&lower, &mut visit),
                            }
                            assert_eq!(
                                got,
                                want.iter().take(limit).cloned().collect::<Vec<_>>(),
                                "degree={degree} descending={descending} lower={lower:?} upper={upper:?} mode={mode} limit={limit}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn descending_iteration() {
    for degree in [2, 3, 8] {
        for descending in [false, true] {
            let less = |a: &(i32, String), b: &(i32, String)| {
                if descending { a.0 > b.0 } else { a.0 < b.0 }
            };
            let mut tree = BTree::new(degree, less);
            tree.descend(|_| panic!("empty tree visited"));
            tree.descend_range(&(0, String::new()), &(10, String::new()), |_| {
                panic!("empty range visited")
            });
            tree.descend_greater_than(&(0, String::new()), |_| panic!("empty tree visited"));
            tree.descend_less_or_equal(&(0, String::new()), |_| panic!("empty tree visited"));
            for key in (0..128).map(|i| ((i * 37) % 128) * 2) {
                tree.replace_or_insert((key, format!("value-{key}")));
            }
            let mut ordered: Vec<_> = (0..128)
                .map(|i| (i * 2, format!("value-{}", i * 2)))
                .collect();
            if !descending {
                ordered.reverse();
            }
            for lower in [-1, 0, 1, 62, 63, 64, 253, 254, 255] {
                for upper in [-1, 0, 1, 62, 63, 64, 253, 254, 255] {
                    let lower = (lower, String::new());
                    let upper = (upper, String::new());
                    for mode in 0..4 {
                        let want: Vec<_> = ordered
                            .iter()
                            .filter(|item| {
                                (mode == 0 || mode == 2 || !less(&lower, item))
                                    && (mode == 0 || mode == 3 || less(&upper, item))
                            })
                            .cloned()
                            .collect();
                        for limit in [1, 3, 17, 256] {
                            let mut got = Vec::new();
                            let mut visit = |item: &(i32, String)| {
                                got.push(item.clone());
                                got.len() < limit
                            };
                            match mode {
                                0 => tree.descend(&mut visit),
                                1 => tree.descend_range(&lower, &upper, &mut visit),
                                2 => tree.descend_greater_than(&upper, &mut visit),
                                _ => tree.descend_less_or_equal(&lower, &mut visit),
                            }
                            assert_eq!(
                                got,
                                want.iter().take(limit).cloned().collect::<Vec<_>>(),
                                "degree={degree} descending={descending} lower={lower:?} upper={upper:?} mode={mode} limit={limit}"
                            );
                        }
                    }
                }
            }
        }
    }
}

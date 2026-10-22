// Copyright 2026 AsterSQL.
//! Topology weights without pooled objects.
use std::collections::HashMap;

use crate::StoreInfo;

#[derive(Default)]
struct Topology<'a> {
    children: HashMap<&'a str, Topology<'a>>,
}

fn exact_value<'a>(store: &'a StoreInfo, key: &str) -> &'a str {
    store
        .labels()
        .iter()
        .find(|label| label.key == key)
        .map_or("", |label| label.value.as_str())
}

/// Calculate a store's share of the eligible topology.
/// Branches use exact label keys and values; location peers use CompareLocation
/// semantics and include ineligible stores. The target contributes one peer even
/// when absent from the input. Count retains Go's signed integer division.
pub fn get_store_topo_weight(
    store: &StoreInfo,
    stores: &[&StoreInfo],
    location_labels: &[&str],
    count: i64,
) -> f64 {
    let mut topology = Topology::default();
    let mut label_count = vec![0_i64; location_labels.len()];
    for candidate in stores {
        if !candidate.is_serving() && !candidate.is_preparing() {
            continue;
        }
        let mut node = &mut topology;
        for (i, label) in location_labels.iter().enumerate() {
            node = node
                .children
                .entry(exact_value(candidate, label))
                .or_insert_with(|| {
                    label_count[i] += 1;
                    Topology::default()
                });
        }
    }
    let mut valid_len = location_labels.len();
    let mut is_match = false;
    for (i, branches) in label_count.into_iter().enumerate() {
        if branches == 0 || count / branches == 0 {
            valid_len = i;
            break;
        }
        if count / branches == 1 && count % branches == 0 {
            valid_len = i + 1;
            is_match = true;
            break;
        }
    }
    let same_location = 1 + stores
        .iter()
        .filter(|candidate| {
            candidate.id() != store.id()
                && store
                    .compare_location(candidate, &location_labels[..valid_len])
                    .is_none()
        })
        .count();
    if is_match {
        return 1.0 / count as f64 / same_location as f64;
    }
    let mut weight = 1.0;
    let mut node = &topology;
    for label in location_labels {
        let Some(child) = node.children.get(exact_value(store, label)) else {
            break;
        };
        // Go slice.Contains uses the label name, including repeated names.
        if location_labels[..valid_len].contains(label) {
            weight /= node.children.len() as f64;
        }
        node = child;
    }
    weight / same_location as f64
}

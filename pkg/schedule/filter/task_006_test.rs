// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{StoreInfo, StoreLabel, StoreMeta, StoreStats};
use astersql_pd_schedule_config::ProviderSnapshot;
use serde_json::Value;
use std::{cmp::Ordering, sync::Arc};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_006.json"
    ))
    .unwrap()
}
fn store(v: &Value) -> Arc<StoreInfo> {
    let mut s = StoreInfo::new(StoreMeta {
        id: v["id"].as_u64().unwrap(),
        labels: v["labels"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| StoreLabel {
                        key: k.clone(),
                        value: v.as_str().unwrap().into(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        ..Default::default()
    });
    if !v["size"].is_null() {
        s.set_region_size(v["size"].as_i64().unwrap());
        s.set_region_weight(v["weight"].as_f64().unwrap());
        s.update_raw_stats(StoreStats {
            capacity: v["capacity"].as_u64().unwrap(),
            available: v["available"].as_u64().unwrap(),
            used_size: v["used"].as_u64().unwrap(),
            ..Default::default()
        });
    }
    Arc::new(s)
}
fn candidates(v: &Value) -> StoreCandidates {
    StoreCandidates::new(
        v.as_array()
            .unwrap()
            .iter()
            .map(|id| store(&serde_json::json!({"id":id})))
            .collect(),
    )
}
fn ids(c: &StoreCandidates) -> Vec<u64> {
    c.pick_all().iter().map(|s| s.id()).collect()
}
fn expected(v: &Value) -> Vec<u64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect()
}
fn by_id(a: &StoreInfo, b: &StoreInfo) -> Ordering {
    a.id().cmp(&b.id())
}
fn by_group(a: &StoreInfo, b: &StoreInfo) -> Ordering {
    (a.id() / 10).cmp(&(b.id() / 10))
}

#[test]
fn task_006_behavior_cases() {
    let f = fixture();
    let mut conf = ProviderSnapshot::<u64>::default();
    conf.schedule.region_score_formula_version =
        f["region_config"]["version"].as_str().unwrap().into();
    conf.schedule.high_space_ratio = f["region_config"]["high"].as_f64().unwrap();
    conf.schedule.low_space_ratio = f["region_config"]["low"].as_f64().unwrap();
    let region = region_score_comparer(&conf);
    let cases = f["comparers"].as_array().unwrap();
    assert_eq!(cases.len(), 23);
    for c in cases {
        let a = store(&c["a"]);
        let b = store(&c["b"]);
        let cmp = if c["kind"] == "RegionScore" {
            for (i, store) in [&a, &b].iter().enumerate() {
                let score = store.region_score("v2", 0.7, 0.8, 0);
                let want = c["scores"][i].as_f64().unwrap();
                assert!((score - want).abs() <= want.abs().max(1.0) * 1e-12);
            }
            region(&a, &b)
        } else {
            let refs: Vec<_> = c["region_stores"]
                .as_array()
                .unwrap()
                .iter()
                .map(store)
                .collect();
            let stores: Vec<_> = refs.iter().map(AsRef::as_ref).collect();
            let labels: Vec<_> = c["labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            for (i, store) in [&a, &b].iter().enumerate() {
                assert_eq!(
                    astersql_pd_core::distinct_score(&labels, &stores, store),
                    c["scores"][i].as_f64().unwrap()
                );
            }
            isolation_comparer(&labels, &stores)(&a, &b)
        };
        assert_eq!(cmp as i8 as i64, c["expected"].as_i64().unwrap(), "{c}");
    }
    // The debugger also ran the actual Go closure with b=a and observed return 0.
    let s = store(&f["region_tie"]["a"]);
    assert_eq!(region(&s, &s), Ordering::Equal);
    let c = &f["candidates"];
    let mut list = candidates(&c["input"]);
    assert_eq!(
        list.pick_the_top_store(by_id, true).unwrap().id(),
        c["minimum"].as_u64().unwrap()
    );
    assert_eq!(
        list.pick_the_top_store(by_id, false).unwrap().id(),
        c["maximum"].as_u64().unwrap()
    );
    list.sort(by_id);
    assert_eq!(ids(&list), expected(&c["sorted"]));
    assert_eq!(list.pick_first().unwrap().id(), 1);
    for asc in [false, true] {
        let mut list = candidates(&c["top_input"]);
        list.keep_the_top_stores(by_group, asc);
        assert_eq!(
            ids(&list),
            expected(&c[if asc { "top_asc" } else { "top_desc" }])
        );
    }
}

#[test]
fn task_006_random_replay_and_faults() {
    let f = fixture();
    let c = &f["candidates"];
    let mut list = candidates(&c["sorted"]);
    let mut tape = c["shuffle_tape"].as_array().unwrap().iter();
    list.shuffle(|bound| {
        let v = tape.next().unwrap();
        assert_eq!(bound, v[0].as_u64().unwrap() as usize + 1);
        v[1].as_u64().unwrap() as usize
    })
    .unwrap();
    assert!(tape.next().is_none());
    assert_eq!(ids(&list), expected(&c["shuffled"]));
    list.sort(by_id);
    let picked = list
        .random_pick(|bound| {
            assert_eq!(bound, 7);
            c["random_index"].as_u64().unwrap() as usize
        })
        .unwrap()
        .unwrap();
    assert_eq!(picked.id(), c["random_pick"].as_u64().unwrap());
    assert!(Arc::ptr_eq(
        picked,
        &list.pick_all()[c["random_index"].as_u64().unwrap() as usize]
    ));
    let original = ids(&list);
    assert_eq!(
        list.random_pick(|n| n).unwrap_err(),
        RandomIndexError {
            index: 7,
            upper_bound: 7
        }
    );
    assert_eq!(
        list.shuffle(|n| n).unwrap_err(),
        RandomIndexError {
            index: 7,
            upper_bound: 7
        }
    );
    assert_eq!(ids(&list), original);
    let mut samples = [0, 6].into_iter();
    assert_eq!(
        list.shuffle(|_| samples.next().unwrap()).unwrap_err(),
        RandomIndexError {
            index: 6,
            upper_bound: 6
        }
    );
    let mut prefix = original;
    prefix.swap(6, 0);
    assert_eq!(ids(&list), prefix);
}

#[test]
fn task_006_boundaries_and_filter_chain() {
    let mut empty = StoreCandidates::default();
    assert!(empty.pick_first().is_none());
    assert!(empty.pick_the_top_store(by_id, true).is_none());
    assert!(
        empty
            .random_pick(|_| panic!("empty must not sample"))
            .unwrap()
            .is_none()
    );
    empty.shuffle(|_| panic!("empty must not sample")).unwrap();
    empty.sort(by_id).keep_the_top_stores(by_id, true);
    assert_eq!(empty.len(), 0);
    let mut single = candidates(&serde_json::json!([u64::MAX]));
    let identity = single.pick_first().unwrap().clone();
    single
        .shuffle(|_| panic!("singleton shuffle must not sample"))
        .unwrap();
    single.keep_the_top_stores(by_id, false);
    assert!(Arc::ptr_eq(&identity, single.pick_first().unwrap()));
    assert_eq!(
        single
            .random_pick(|n| {
                assert_eq!(n, 1);
                0
            })
            .unwrap()
            .unwrap()
            .id(),
        u64::MAX
    );
    let mut list = candidates(&serde_json::json!([23, 10, 15, 20, 11]));
    let original = list.pick_all().to_vec();
    assert!(Arc::ptr_eq(
        list.pick_the_top_store(by_group, true).unwrap(),
        &original[1]
    ));
    assert_eq!(ids(&list), vec![23, 10, 15, 20, 11]);
    list.keep_the_top_stores(by_group, true);
    assert_eq!(ids(&list), vec![10, 15, 11]);
    let conf = ProviderSnapshot::<u64>::default();
    let filter = ExcludedFilter::new("task006", [15], [11]);
    list.filter_source(&conf, None, &[&filter])
        .filter_target(&conf, None, &[&filter]);
    assert_eq!(ids(&list), vec![10]);
    assert!(Arc::ptr_eq(&original[1], list.pick_first().unwrap()));
}

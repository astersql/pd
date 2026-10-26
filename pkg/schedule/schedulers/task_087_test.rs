// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_statistics::*;
use serde_json::Value;
fn detail(loads: Loads) -> StoreLoadDetail {
    StoreLoadDetail {
        store_summary_info: Default::default(),
        hot_peers: vec![],
        load_pred: StoreLoadPred {
            current: StoreLoad {
                loads,
                ..Default::default()
            },
            future: StoreLoad {
                loads,
                ..Default::default()
            },
            ..Default::default()
        },
    }
}
fn config() -> HotRegionParams {
    HotRegionParams {
        min_hot_byte_rate: 10.,
        min_hot_key_rate: 10.,
        min_hot_query_rate: 10.,
        min_hot_cpu_rate: 10.,
        ..Default::default()
    }
}
fn ranker(write: bool) -> HotRankV1 {
    HotRankV1::new(
        config(),
        [0, 1],
        write,
        StoreLoad::default(),
        StoreLoad::default(),
        StoreLoad {
            loads: [10.; 4],
            hot_peer_count: 1.,
            ..Default::default()
        },
    )
    .unwrap()
}
fn loads(v: &Value) -> Loads {
    std::array::from_fn(|i| v[i].as_f64().unwrap())
}
#[test]
fn task_087_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_087.json"
    ))
    .unwrap();
    assert_eq!(f["evidence_kind"], "source_assertion");
    assert_eq!(f["cases"].as_array().unwrap().len(), 15);
    for c in f["cases"].as_array().unwrap() {
        let mut src = detail(loads(&c["source"]));
        let mut dst = detail(loads(&c["destination"]));
        if !c["future_source"].is_null() {
            src.load_pred.future.loads = loads(&c["future_source"]);
            dst.load_pred.future.loads = loads(&c["future_destination"]);
        }
        let peer = HotPeerStat {
            loads: loads(&c["peer"]).to_vec(),
            ..Default::default()
        };
        let revert = (!c["revert"].is_null()).then(|| HotPeerStat {
            loads: loads(&c["revert"]).to_vec(),
            ..Default::default()
        });
        let mut s = HotSolution::new(&src, &dst, &peer, revert.as_ref());
        let r = ranker(c["write_leader"].as_bool().unwrap_or(false));
        let rank = r.progressive_rank(&mut s);
        assert_eq!(rank, c["rank"].as_i64().unwrap() as i32, "{}", c["name"]);
        assert_eq!(r.is_available(rank), rank > 0);
        assert!(!r.need_search_revert_regions());
    }
    println!("task_087: 15 frozen rank fixtures");
}
#[test]
fn task_087_priority_history_and_faults() {
    for write in [false, true] {
        for strict in [false, true] {
            let mut r = ranker(write);
            r.config.strict_picking_store = strict;
            let current = StoreLoad {
                loads: [110., 90., 0., 0.],
                history_loads: [vec![110., 120.], vec![90., 120.], vec![], vec![]],
                ..Default::default()
            };
            let expect = StoreLoad {
                loads: [100.; 4],
                history_loads: std::array::from_fn(|_| vec![100., 100.]),
                ..Default::default()
            };
            let expected = write || !strict;
            assert_eq!(r.check_loads(&current, &expect, 1., true), expected);
            assert_eq!(r.check_history(&current, &expect, 1., true), expected);
            let empty = StoreLoad::default();
            assert!(r.check_history(&empty, &empty, 1., true));
            let low = StoreLoad {
                loads: [90., 110., 0., 0.],
                history_loads: [vec![90., 80.], vec![110., 80.], vec![], vec![]],
                ..Default::default()
            };
            assert_eq!(r.check_loads(&low, &expect, 1., false), expected);
            assert_eq!(r.check_history(&low, &expect, 1., false), expected);
            assert!(!r.check_loads(&expect, &expect, 1., true));
            assert!(!r.check_loads(&expect, &expect, 1., false));
            let bad = StoreLoad {
                history_loads: [vec![110.], vec![110.], vec![], vec![]],
                ..Default::default()
            };
            assert!(std::panic::catch_unwind(|| r.check_history(&bad, &empty, 1., true)).is_err());
        }
    }
    assert_eq!(
        HotRankV1::new(
            config(),
            [4, 0],
            false,
            Default::default(),
            Default::default(),
            Default::default()
        )
        .err(),
        Some(HotRankV1Error::InvalidPriorities)
    );
    assert_eq!(
        HotRankV1::new(
            config(),
            [1, 1],
            false,
            Default::default(),
            Default::default(),
            Default::default()
        )
        .err(),
        Some(HotRankV1Error::InvalidPriorities)
    );
    // CPU/query priorities use the same actual domain loads and history arrays.
    let r = HotRankV1::new(
        config(),
        [3, 2],
        false,
        Default::default(),
        Default::default(),
        Default::default(),
    )
    .unwrap();
    let src = detail([0., 0., 100., 100.]);
    let dst = detail([0., 0., 20., 20.]);
    let p = HotPeerStat {
        loads: vec![0., 0., 20., 20.],
        ..Default::default()
    };
    assert_eq!(
        r.progressive_rank(&mut HotSolution::new(&src, &dst, &p, None)),
        4
    );
}
#[test]
fn task_087_uniform_and_labels() {
    let mut r = ranker(false);
    let mut src = detail([100.; 4]);
    src.load_pred.stddev.loads = [0.04, 0.09, 0., 0.];
    assert_eq!(r.filter_uniform_store(&src, 4), Some("all-dim"));
    src.load_pred.stddev.loads = [0.04, 0.1, 0., 0.];
    for rank in [-1, 0, 1, 2, 3, 4, 5] {
        assert_eq!(
            r.filter_uniform_store(&src, rank),
            if rank == 1 || rank == 3 {
                Some("byte")
            } else {
                None
            }
        );
    }
    src.load_pred.stddev.loads = [0.05, 0.09, 0., 0.];
    assert_eq!(r.filter_uniform_store(&src, 2), Some("key"));
    assert_eq!(r.filter_uniform_store(&src, 4), None);
    r.config.src_tolerance_ratio = 0.;
    assert_eq!(r.filter_uniform_store(&src, 2), None);
    for (rank, label) in [
        (-1, "none"),
        (0, "none"),
        (1, "byte-only"),
        (2, "key"),
        (3, "byte"),
        (4, "all"),
        (5, "none"),
    ] {
        assert_eq!(r.rank_to_dim_string(rank), label);
    }
}
#[test]
fn task_087_candidate_ordering() {
    let r = ranker(false);
    let src = detail([1000.; 4]);
    let dst = detail([10.; 4]);
    let p = HotPeerStat {
        loads: vec![300., 30., 0., 0.],
        ..Default::default()
    };
    let q = HotPeerStat {
        loads: vec![200., 20., 0., 0.],
        ..Default::default()
    };
    let mut s = HotSolution::new(&src, &dst, &p, None);
    s.calc_peers_rate(&[0, 1]);
    let mut old = HotSolution::new(&src, &dst, &q, None);
    old.calc_peers_rate(&[0, 1]);
    assert!(r.better_than(&s, 4, None));
    assert!(r.better_than(&s, 5, Some((&old, 6))));
    assert!(r.better_than(&s, 4, Some((&old, 3))));
    assert!(!r.better_than(&s, 3, Some((&old, 4))));
    for (rank, yes) in [
        (4, true),
        (3, true),
        (2, true),
        (1, true),
        (0, false),
        (-1, false),
    ] {
        assert_eq!(r.better_than(&s, rank, Some((&old, rank))), yes);
    }
    for rank in [-1, 0, 1, 2, 3, 4] {
        assert!(!r.better_than(&s, rank, Some((&s, rank))));
    }
    let mut revert = HotSolution::new(&src, &dst, &p, Some(&q));
    revert.calc_peers_rate(&[0, 1]);
    assert!(r.better_than(&s, 3, Some((&revert, 3))));
    assert!(!r.better_than(&revert, 3, Some((&s, 3))));
    // Rank 3 secondary rate decreases oscillation; rank 2 primary rate does likewise.
    let t = HotPeerStat {
        loads: vec![300., 20., 0., 0.],
        ..Default::default()
    };
    let mut equal_first = HotSolution::new(&src, &dst, &t, None);
    equal_first.calc_peers_rate(&[0, 1]);
    assert!(r.better_than(&equal_first, 3, Some((&s, 3))));
    assert!(r.better_than(&old, 2, Some((&equal_first, 2))));
    let w = ranker(true);
    assert!(w.better_than(&s, 3, Some((&old, 3))));
    // Source and destination selection precedes peer rates, using discretized load predictions.
    let larger = detail([1100.; 4]);
    let smaller = detail([0.; 4]);
    let mut a = HotSolution::new(&larger, &dst, &q, None);
    a.calc_peers_rate(&[0, 1]);
    assert!(r.better_than(&a, 4, Some((&s, 4))));
    assert!(!r.better_than(&s, 4, Some((&a, 4))));
    let mut b = HotSolution::new(&src, &smaller, &q, None);
    b.calc_peers_rate(&[0, 1]);
    assert!(r.better_than(&b, 4, Some((&s, 4))));
    println!("task_087: rank, revert, source, destination, peer, identity tie ordering");
}

#[test]
fn task_087_ratio_and_tolerance_boundaries() {
    let mut r = ranker(false);
    let src = detail([100.; 4]);
    let dst = detail([20.; 4]);
    let p = HotPeerStat {
        loads: vec![20.; 4],
        ..Default::default()
    };
    let mut s = HotSolution::new(&src, &dst, &p, None);
    s.calc_peers_rate(&[0, 1]);
    assert_eq!(r.hot_decline_ratio(&s, 0), (true, 0.5));
    r.config.great_dec_ratio = 0.5;
    assert_eq!(r.progressive_rank(&mut s), 4);
    r.config.great_dec_ratio = 0.499;
    assert_eq!(r.progressive_rank(&mut s), -1);
    let zero = HotPeerStat {
        loads: vec![0., 20., 0., 0.],
        ..Default::default()
    };
    let mut s = HotSolution::new(&src, &dst, &zero, None);
    // first dim is not hot, even when its ratio exceeds minor threshold.
    assert_eq!(r.progressive_rank(&mut s), -1);
    r.config.great_dec_ratio = 0.95;
    assert_eq!(r.progressive_rank(&mut s), 2);
    let negative = HotPeerStat {
        loads: vec![40., 0., 0., 0.],
        ..Default::default()
    };
    let mut s = HotSolution::new(&src, &dst, &p, Some(&negative));
    s.calc_peers_rate(&[0, 1]);
    assert_eq!(r.hot_decline_ratio(&s, 0), (true, 120.)); // denominator is clamped to 1
    assert_eq!(r.progressive_rank(&mut s), 0); // positive second bypasses signed early cutoff
    let mut source = detail([100.; 4]);
    let mut target = detail([20.; 4]);
    // amp=3.5, exactly 80/3.5 pending difference fails strict tolerance.
    source.load_pred.future.loads = [100. - 80. / 3.5; 4];
    let mut s = HotSolution::new(&source, &target, &p, None);
    s.calc_peers_rate(&[0, 1]);
    assert!(!r.is_tolerance(&s, 0));
    source.load_pred.future.loads = [80.; 4];
    let s = HotSolution::new(&source, &target, &p, None);
    assert!(r.is_tolerance(&s, 0));
    target.load_pred.future.loads = [25.; 4];
    let s = HotSolution::new(&source, &target, &p, None);
    assert!(!r.is_tolerance(&s, 0));
    assert!(!r.is_tolerance(&HotSolution::new(&target, &source, &p, None), 0));
    r.set_search_revert_regions();
    assert!(!r.need_search_revert_regions());
}

#[test]
fn task_087_discrete_store_and_peer_ties() {
    let r = ranker(false);
    let w = ranker(true);
    let mut a = detail([100.; 4]);
    let mut b = a.clone();
    let dst = detail([0.; 4]);
    let p = HotPeerStat {
        loads: vec![199., 19., 0., 0.],
        ..Default::default()
    };
    let q = HotPeerStat {
        loads: vec![100., 10., 0., 0.],
        ..Default::default()
    };
    let compare = |r: &HotRankV1, a: &StoreLoadDetail, b: &StoreLoadDetail, rank| {
        let mut s = HotSolution::new(a, &dst, &p, None);
        let mut old = HotSolution::new(b, &dst, &q, None);
        s.calc_peers_rate(&[0, 1]);
        old.calc_peers_rate(&[0, 1]);
        r.better_than(&s, rank, Some((&old, rank)))
    };
    // Peer rates in the same 100-byte/10-key bucket tie in regular mode.
    for rank in [1, 2, 3, 4] {
        assert!(!compare(&r, &a, &b, rank));
    }
    assert!(compare(&w, &a, &b, 3)); // write leader uses raw first rate
    a.load_pred.future.loads = [120.; 4];
    b.load_pred.future.loads = [130.; 4];
    assert!(compare(&r, &a, &b, 4)); // same min, smaller first pending wins
    a.load_pred.future.loads = [130.; 4];
    a.load_pred.future.hot_peer_count = 1.;
    b.load_pred.future.hot_peer_count = 2.;
    assert!(compare(&w, &a, &b, 3)); // peer count before pending rate for write leader
    assert!(!compare(&r, &a, &b, 4)); // count deliberately ignored for other modes
    // Store rank buckets use subtraction before truncating toward zero, including negative values.
    let r = HotRankV1::new(
        config(),
        [0, 1],
        false,
        StoreLoad {
            loads: [105.; 4],
            ..Default::default()
        },
        Default::default(),
        StoreLoad {
            loads: [10.; 4],
            ..Default::default()
        },
    )
    .unwrap();
    let a = detail([104.; 4]);
    let b = detail([96.; 4]);
    assert!(!compare(&r, &a, &b, 4));
    let b = detail([95.; 4]);
    assert!(compare(&r, &a, &b, 4));
    // Destination max and its pending tie-break are independent of source.
    let src = detail([200.; 4]);
    let mut a = detail([20.; 4]);
    let mut b = detail([10.; 4]);
    a.load_pred.future.loads = [30.; 4];
    b.load_pred.future.loads = [30.; 4];
    let mut s = HotSolution::new(&src, &a, &q, None);
    let mut old = HotSolution::new(&src, &b, &p, None);
    s.calc_peers_rate(&[0, 1]);
    old.calc_peers_rate(&[0, 1]);
    assert!(r.better_than(&s, 4, Some((&old, 4))));
}

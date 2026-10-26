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
fn ranker(write: bool) -> HotRankV2 {
    HotRankV2::new(
        HotRegionParams {
            great_dec_ratio: 0.95,
            min_hot_byte_rate: 1.,
            min_hot_key_rate: 1.,
            min_hot_query_rate: 1.,
            min_hot_cpu_rate: 1.,
            ..Default::default()
        },
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
fn peer(loads: Loads) -> HotPeerStat {
    HotPeerStat {
        loads: loads.to_vec(),
        ..Default::default()
    }
}
#[test]
fn task_088_behavior_cases() {
    let f: Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_088.json"
    ))
    .unwrap();
    assert_eq!(f["evidence_kind"], "source_assertion");
    let r = ranker(false);
    for c in f["scores"].as_array().unwrap() {
        let src = detail([c["high"].as_f64().unwrap(); 4]);
        let dst = detail([c["low"].as_f64().unwrap(); 4]);
        let p = peer([c["peer"].as_f64().unwrap(); 4]);
        let mut s = HotSolution::new(&src, &dst, &p, None);
        s.calc_peers_rate(&[0, 1]);
        assert_eq!(
            r.score(&s, 0, RankRatios::new(0.95, 0.2, 0.02), None),
            c["score"].as_i64().unwrap() as i32,
            "{c}"
        );
    }
    let src = detail([1000.; 4]);
    let dst = detail([500.; 4]);
    for (i, a) in [200., 0., 400.].into_iter().enumerate() {
        for (j, b) in [200., 0., 400.].into_iter().enumerate() {
            let p = peer([a, b, 0., 0.]);
            let mut s = HotSolution::new(&src, &dst, &p, None);
            let result = r.progressive_rank(&mut s, [None; 2], [None; 2]);
            assert_eq!(
                result.rank,
                f["rank_matrix"][i][j].as_i64().unwrap() as i32,
                "{i}/{j}"
            );
        }
    }
    println!("22 score fixtures and 9 rank matrix cases");
}
#[test]
fn task_088_ratios_pending_reverse_and_topn() {
    for (input, want) in [(0., 0.7), (0.7, 0.7), (0.95, 0.95), (1., 0.95)] {
        let x = RankRatios::new(input, 0.2, 0.02);
        assert_eq!(x.future_balanced, want);
        assert!((x.current_balanced - (want - 0.02)).abs() < 1e-12);
    }
    let r = ranker(false);
    let rs = r.ratios[0];
    let src = detail([1000.; 4]);
    let dst = detail([500.; 4]);
    let p = peer([2.; 4]);
    let mut s = HotSolution::new(&src, &dst, &p, None);
    s.calc_peers_rate(&[0, 1]);
    assert_eq!(r.score(&s, 0, rs, None), 0);
    assert_eq!(r.score(&s, 0, rs, Some(1.)), 2);
    // A negative net rate uses reversed extrema/current/pending and the high destination's topn.
    let main = peer([200., 200., 0., 0.]);
    let back = peer([202., 0., 0., 0.]);
    let mut reverse = HotSolution::new(&dst, &src, &main, Some(&back));
    let res = r.progressive_rank(&mut reverse, [None; 2], [Some(1.), None]);
    assert_eq!(res.scores[0], 2);
    assert!(r.is_tolerance(&reverse, 0, true));
    // Pre-balanced positive score requires absolute pending sum < 1.
    let mut high = detail([1000.; 4]);
    let low = detail([900.; 4]);
    let p = peer([50.; 4]);
    high.load_pred.future.loads = [999.5; 4];
    let mut s = HotSolution::new(&high, &low, &p, None);
    s.calc_peers_rate(&[0, 1]);
    assert_eq!(r.score(&s, 0, rs, None), 3);
    high.load_pred.future.loads = [999.; 4];
    let mut s = HotSolution::new(&high, &low, &p, None);
    s.calc_peers_rate(&[0, 1]);
    assert_eq!(r.score(&s, 0, rs, None), 0);
    high.load_pred.future.loads = [900.; 4];
    let mut s = HotSolution::new(&high, &low, &p, None);
    s.calc_peers_rate(&[0, 1]);
    assert!(!r.is_tolerance(&s, 0, false));
    // Exact balance interval endpoints are inclusive.
    let high = detail([1000.; 4]);
    let low = detail([500.; 4]);
    for rate in [(1000. * 0.95 - 500.) / 1.95, (1000. - 500. * 0.95) / 1.95] {
        let p = peer([rate; 4]);
        let mut s = HotSolution::new(&high, &low, &p, None);
        s.calc_peers_rate(&[0, 1]);
        assert_eq!(r.score(&s, 0, rs, None), 3);
    }
    let write = ranker(true);
    let p = peer([200., 400., 0., 0.]);
    let mut s = HotSolution::new(&high, &low, &p, None);
    assert_eq!(
        write.progressive_rank(&mut s, [None; 2], [None; 2]),
        HotRankV2Result {
            rank: 3,
            scores: [0, 0]
        }
    );
}
#[test]
fn task_088_history_faults_and_uniform() {
    let mut r = ranker(false);
    let mut load = StoreLoad {
        loads: [90., 110., 0., 0.],
        history_loads: [vec![90.], vec![110.], vec![], vec![]],
        ..Default::default()
    };
    let expect = StoreLoad {
        loads: [100.; 4],
        history_loads: std::array::from_fn(|_| vec![100.]),
        ..Default::default()
    };
    for source in [true, false] {
        assert!(r.check_loads(&load, &expect, 1., source));
        assert!(r.check_history(&load, &expect, 1., source));
    }
    assert!(!ranker(true).check_loads(&load, &expect, 1., true));
    assert!(!ranker(true).check_history(&load, &expect, 1., true));
    assert!(!r.check_loads(&expect, &expect, 1., true));
    assert!(r.check_history(&StoreLoad::default(), &StoreLoad::default(), 1., true));
    load.history_loads[0] = vec![110., 120.];
    assert!(std::panic::catch_unwind(|| r.check_history(&load, &expect, 1., true)).is_err());
    assert!(
        HotRankV2::new(
            r.config.clone(),
            [0, 0],
            false,
            StoreLoad::default(),
            StoreLoad::default(),
            StoreLoad::default()
        )
        .is_err()
    );
    assert!(
        HotRankV2::new(
            r.config.clone(),
            [4, 0],
            false,
            StoreLoad::default(),
            StoreLoad::default(),
            StoreLoad::default()
        )
        .is_err()
    );
    let source = detail([100.; 4]);
    let both = StoreLoad {
        loads: [0.01; 4],
        ..Default::default()
    };
    let first = StoreLoad {
        loads: [0.049, 0.1, 0., 0.],
        ..Default::default()
    };
    let second = StoreLoad {
        loads: [0.05, 0.099, 0., 0.],
        ..Default::default()
    };
    let neither = StoreLoad {
        loads: [0.05, 0.1, 0., 0.],
        ..Default::default()
    };
    assert_eq!(
        r.filter_uniform_store(&source, 4, Some(&both), None, false),
        Some("all-dim")
    );
    for rank in -1..=5 {
        assert_eq!(
            r.filter_uniform_store(&source, rank, Some(&first), None, false),
            if [2, 3].contains(&rank) {
                Some("byte")
            } else {
                None
            }
        );
        assert_eq!(
            r.filter_uniform_store(&source, rank, Some(&second), None, false),
            if rank == 1 { Some("key") } else { None }
        );
        assert_eq!(
            r.filter_uniform_store(&source, rank, Some(&neither), None, false),
            None
        );
    }
    assert_eq!(
        r.filter_uniform_store(&source, 4, Some(&neither), Some(&both), true),
        Some("all-dim")
    );
    assert_eq!(
        r.filter_uniform_store(&source, 4, Some(&neither), Some(&both), false),
        None
    );
    r.config.src_tolerance_ratio = 0.;
    assert_eq!(
        r.filter_uniform_store(&source, 4, Some(&both), None, false),
        None
    );
    for (rank, label) in [
        (4, "all"),
        (3, "byte"),
        (2, "byte-only"),
        (1, "key"),
        (0, "none"),
    ] {
        assert_eq!(r.rank_to_dim_string(rank), label);
    }
}
#[test]
fn task_088_revert_state_and_candidate_ties() {
    let mut r = ranker(false);
    for best in [
        None,
        Some((2, false)),
        Some((4, true)),
        Some((4, false)),
        Some((1, false)),
    ] {
        r.set_search_revert_regions(best);
        assert_eq!(
            r.search_revert_regions,
            best.is_none_or(|(n, b)| n == 2 || b)
        );
        for current in -1..=5 {
            assert_eq!(
                r.need_search_revert_regions(current, best),
                r.search_revert_regions
                    && [0, 2].contains(&current)
                    && best.is_none_or(|(n, b)| n <= 2 || b)
            );
        }
    }
    for rank in -1..=5 {
        for revert in [false, true] {
            assert_eq!(
                HotRankV2Result {
                    rank,
                    scores: [0; 2]
                }
                .is_available(revert),
                rank >= 3 || rank > 0 && !revert
            );
        }
    }
    for d in 0..4 {
        for score in -2..=3 {
            assert_eq!(
                v2_priority_cmp(d, score, score, 300., 100.),
                if score == 2 { 1 } else { -1 }
            );
            assert_eq!(v2_priority_cmp(d, score, score, 101., 100.), 0);
            assert_eq!(v2_priority_cmp(d, score + 1, score, 0., 1000.), 1);
        }
    }
    let high = detail([1000.; 4]);
    let low = detail([500.; 4]);
    let p = peer([300., 200., 0., 0.]);
    let old_peer = peer([100., 100., 0., 0.]);
    let mut s = HotSolution::new(&high, &low, &p, None);
    let mut old = HotSolution::new(&high, &low, &old_peer, None);
    s.calc_peers_rate(&[0, 1]);
    old.calc_peers_rate(&[0, 1]);
    for rank in 1..=4 {
        for score in -2..=3 {
            let res = HotRankV2Result {
                rank,
                scores: [score; 2],
            };
            assert_eq!(r.better_than(&s, res, Some((&old, res))), score == 2);
            assert!(!r.better_than(&s, res, Some((&s, res))));
            assert!(r.better_than(&s, res, None));
        }
    }
    let res = HotRankV2Result {
        rank: 4,
        scores: [3; 2],
    };
    let mut reverted = HotSolution::new(&high, &low, &old_peer, Some(&p));
    reverted.calc_peers_rate(&[0, 1]);
    assert!(r.better_than(&s, res, Some((&reverted, res))));
    assert!(!r.better_than(&reverted, res, Some((&s, res))));
    assert!(r.better_than(&s, HotRankV2Result { rank: 5, ..res }, Some((&old, res))));
    assert!(!r.better_than(&s, HotRankV2Result { rank: 0, ..res }, Some((&old, res))));
    let higher = detail([1100.; 4]);
    let mut preferred = HotSolution::new(&higher, &low, &p, None);
    preferred.calc_peers_rate(&[0, 1]);
    assert!(r.better_than(&preferred, res, Some((&s, res))));
    let lower = detail([400.; 4]);
    let mut preferred = HotSolution::new(&high, &lower, &p, None);
    preferred.calc_peers_rate(&[0, 1]);
    assert!(r.better_than(&preferred, res, Some((&s, res))));
}
#[test]
fn task_088_priority_order_and_real_population() {
    let r = ranker(false);
    let high = detail([1000.; 4]);
    let low = detail([500.; 4]);
    let a = peer([300., 100., 0., 0.]);
    let b = peer([100., 300., 0., 0.]);
    let mut s = HotSolution::new(&high, &low, &a, None);
    let mut old = HotSolution::new(&high, &low, &b, None);
    s.calc_peers_rate(&[0, 1]);
    old.calc_peers_rate(&[0, 1]);
    for rank in 1..=4 {
        let res = HotRankV2Result {
            rank,
            scores: [2; 2],
        };
        assert_eq!(r.better_than(&s, res, Some((&old, res))), rank != 1);
        // Higher first score trumps an adverse rate bucket when first is decisive.
        let higher = HotRankV2Result {
            rank,
            scores: [3, 1],
        };
        assert_eq!(r.better_than(&s, higher, Some((&old, res))), rank != 1);
    }
    let a = peer([100., 300., 0., 0.]);
    let b = peer([101., 100., 0., 0.]);
    let mut s = HotSolution::new(&high, &low, &a, None);
    let mut old = HotSolution::new(&high, &low, &b, None);
    s.calc_peers_rate(&[0, 1]);
    old.calc_peers_rate(&[0, 1]);
    for rank in 1..=4 {
        let res = HotRankV2Result {
            rank,
            scores: [2; 2],
        };
        assert!(r.better_than(&s, res, Some((&old, res))));
    }
    let write = ranker(true);
    let res = HotRankV2Result {
        rank: 3,
        scores: [0; 2],
    };
    assert!(!write.better_than(&s, res, Some((&old, res)))); // same first bucket, second ignored
    let r = HotRankV2::new(
        r.config.clone(),
        [3, 2],
        false,
        StoreLoad::default(),
        StoreLoad::default(),
        StoreLoad {
            loads: [10.; 4],
            ..Default::default()
        },
    )
    .unwrap();
    let p = peer([0., 0., 200., 200.]);
    let mut s = HotSolution::new(&high, &low, &p, None);
    assert_eq!(r.progressive_rank(&mut s, [None; 2], [None; 2]).rank, 4);
    assert_eq!(r.rank_to_dim_string(2), "cpu-only");
    // Scope statistics are computed with the canonical population accumulator.
    let population = [detail([100.; 4]), detail([100.; 4])];
    let mut summary = StoreLoadSummary::default();
    for store in &population {
        summary.add(&store.load_pred.current);
    }
    let (_, stddev) = summary.result(&population);
    assert_eq!(
        r.filter_uniform_store(&high, 4, Some(&stddev), None, false),
        Some("all-dim")
    );
    let s = HotSolution::new(&high, &low, &p, None);
    assert!(std::panic::catch_unwind(|| r.score(&s, 3, r.ratios[0], None)).is_err());
}

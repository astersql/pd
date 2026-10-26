// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_core::{
    BasicCluster, KeyRange, Peer, PeerRole, RegionEpoch, RegionInfo, RegionMeta, RegionsInfo,
};
use astersql_pd_schedule_hbstream::SplitPolicy;
use astersql_pd_schedule_operator::{
    BuilderError, ControllerAdmission, OpKind, OpStep, SplitRegion,
};
use std::sync::Arc;
fn region(id: u64, start: &str, end: &str, version: u64) -> Arc<RegionInfo> {
    let peer = Peer {
        id: 1,
        store_id: 1,
        ..Default::default()
    };
    Arc::new(RegionInfo::new(
        RegionMeta {
            id,
            key_range: KeyRange {
                start_key: start.as_bytes().to_vec(),
                end_key: end.as_bytes().to_vec(),
            },
            epoch: Some(RegionEpoch {
                version,
                conf_ver: 2,
            }),
            peers: vec![peer.clone()],
            ..Default::default()
        },
        Some(peer),
        vec![],
        vec![],
    ))
}
fn group(keys: &[&str]) -> RegionGroupKeys {
    RegionGroupKeys {
        region: region(1, "a", "z", 2),
        keys: keys.iter().map(|k| k.as_bytes().to_vec()).collect(),
    }
}
#[test]
fn task_063_behavior_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/rust/schedule-execution/fixtures/task_063.json"
    ))
    .unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let mut regions = RegionsInfo::new();
        for r in c["ranges"].as_array().unwrap() {
            regions.set_region(region(
                r[0].as_u64().unwrap(),
                r[1].as_str().unwrap(),
                r[2].as_str().unwrap(),
                r[3].as_u64().unwrap(),
            ));
        }
        let keys: Vec<_> = c["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        let mut progress = SplitGroupProgress::new(group(&keys));
        let mut results = SplitKeyResults::default();
        progress.scan_regions_by_key_range(&regions, &mut results);
        let found: std::collections::BTreeMap<_, _> = results
            .regions()
            .iter()
            .map(|(id, k)| (id.to_string(), String::from_utf8(k.clone()).unwrap()))
            .collect();
        assert_eq!(
            serde_json::to_value(found).unwrap(),
            c["found"],
            "{} found",
            c["id"]
        );
        assert_eq!(
            progress.finished(),
            c["finished"].as_bool().unwrap(),
            "{} finished",
            c["id"]
        );
        let remaining: Vec<_> = results
            .unprocessed_keys(&progress.group.keys)
            .into_iter()
            .map(|k| String::from_utf8(k).unwrap())
            .collect();
        assert_eq!(
            serde_json::to_value(remaining).unwrap(),
            c["remaining"],
            "{} remaining",
            c["id"]
        );
    }
    println!("task_063: 9 frozen source-derived scan cases");
}
#[test]
fn task_063_real_admission_and_heartbeat_command() {
    let mut cluster = BasicCluster::new();
    let g = group(&["m", "t"]);
    cluster.regions.set_region(g.region.clone());
    let ctl = ControllerAdmission::new(10);
    let outcome = split_region_by_keys(&cluster, &ctl, &g).unwrap();
    assert!(outcome.removed.is_empty());
    let op = ctl.get_operator(1).unwrap();
    assert_eq!(outcome.admitted.len(), 1);
    assert!(Arc::ptr_eq(&op, &outcome.admitted[0]));
    assert_eq!(op.desc(), "region-splitter");
    assert_eq!(op.kind(), OpKind::SPLIT);
    assert_eq!(op.region_epoch(), g.region.meta().epoch.as_ref());
    let Some(OpStep::SplitRegion(step)) = op.step(0) else {
        panic!("split step")
    };
    assert_eq!(
        step,
        &SplitRegion {
            start_key: b"a".to_vec(),
            end_key: b"z".to_vec(),
            policy: SplitPolicy::UseKey,
            split_keys: g.keys.clone()
        }
    );
    let command = op
        .step(0)
        .unwrap()
        .get_cmd(&g.region, false)
        .unwrap()
        .split_region
        .unwrap();
    assert_eq!(command.policy, SplitPolicy::UseKey);
    assert_eq!(command.keys, g.keys);
    assert!(matches!(
        split_region_by_keys(&cluster, &ctl, &g),
        Err(SplitExecutionError::Admission(_))
    ));
    let mut p = SplitGroupProgress::new(g);
    let mut results = SplitKeyResults::default();
    p.scan_regions_by_key_range(&cluster.regions, &mut results);
    assert!(results.regions().is_empty());
    assert_eq!(results.unprocessed_keys(&p.group.keys), p.group.keys);
    assert!(!p.finished());
}
#[test]
fn task_063_factory_and_stale_epoch_failures() {
    let mut c = BasicCluster::new();
    c.regions.set_region(region(1, "a", "z", 3));
    let ctl = ControllerAdmission::new(1);
    assert!(matches!(
        split_region_by_keys(&c, &ctl, &group(&["m"])),
        Err(SplitExecutionError::Admission(_))
    ));
    assert!(ctl.get_operator(1).is_none());
    let mut g = group(&["m"]);
    let mut meta = g.region.meta().clone();
    meta.peers[0].role = PeerRole::IncomingVoter;
    g.region = Arc::new(RegionInfo::new(meta, None, vec![], vec![]));
    assert!(matches!(
        split_region_by_keys(&c, &ctl, &g),
        Err(SplitExecutionError::Build(BuilderError::OriginJoint))
    ));
    assert!(ctl.get_operator(1).is_none());
}
#[test]
fn task_063_partial_accumulation_and_finished_latch() {
    let mut p = SplitGroupProgress::new(group(&["m", "t"]));
    let mut r = RegionsInfo::new();
    let mut result = SplitKeyResults::default();
    r.set_region(region(2, "a", "m", 3));
    r.set_region(region(3, "m", "z", 3));
    p.scan_regions_by_key_range(&r, &mut result);
    assert_eq!(result.unprocessed_keys(&p.group.keys), vec![b"t".to_vec()]);
    r = RegionsInfo::new();
    r.set_region(region(4, "t", "z", 4));
    p.scan_regions_by_key_range(&r, &mut result);
    assert!(!p.finished()); // completion counts this scan, not accumulated keys
    assert!(result.unprocessed_keys(&p.group.keys).is_empty());
    r.set_region(region(3, "m", "t", 4));
    p.scan_regions_by_key_range(&r, &mut result);
    assert!(p.finished());
    p.scan_regions_by_key_range(&RegionsInfo::new(), &mut result);
    assert!(p.finished());
    assert_eq!(
        result.regions().keys().copied().collect::<Vec<_>>(),
        vec![3, 4]
    );
}
#[tokio::test]
async fn task_063_group_submit_observe_and_suspect() {
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_filter::RegionReplicationContext;
    use astersql_pd_schedule_placement::RuleManager;
    use astersql_pd_storage_endpoint::StorageEndpoint;
    use astersql_pd_storage_kv::MemoryStore;
    use std::cell::RefCell;
    let mut c = BasicCluster::new();
    c.regions.set_region(region(1, "a", "z", 2));
    c.regions.set_region(Arc::new(RegionInfo::new(
        RegionMeta {
            id: 5,
            key_range: KeyRange {
                start_key: b"z".to_vec(),
                end_key: vec![],
            },
            ..Default::default()
        },
        None,
        vec![],
        vec![],
    )));
    let mut conf = ProviderSnapshot::<()>::default();
    conf.replication.enable_placement_rules = false;
    conf.replication.max_replicas = 1;
    let mut rules = RuleManager::new(
        Arc::new(StorageEndpoint::new(Arc::new(MemoryStore::default()))),
        astersql_pd_keypath::ClusterPaths::new(63),
    );
    let context = RegionReplicationContext {
        conf: &conf,
        stores: &[],
        manager: RefCell::new(&mut rules),
        now: 0,
    };
    let keys = vec![b"m".to_vec(), b"t".to_vec(), b"zz".to_vec()];
    let mut suspect = vec![];
    let groups = group_keys_by_region(&c.regions, &context, &keys, |flag, id| {
        suspect.push((flag, id))
    });
    assert_eq!(suspect, vec![(false, 5)]);
    let ctl = ControllerAdmission::new(10);
    let (mut attempt, submissions) = SplitAttempt::submit(&c, &ctl, groups);
    assert_eq!(submissions.admitted.len(), 1);
    assert!(submissions.rejected.is_empty());
    attempt.scan(&c.regions);
    assert!(!attempt.finished());
    assert_eq!(attempt.results().unprocessed_keys(&keys), keys);
    c.regions.set_region(region(2, "a", "m", 3));
    c.regions.set_region(region(3, "m", "t", 3));
    c.regions.set_region(region(4, "t", "z", 3));
    attempt.scan(&c.regions);
    assert!(attempt.finished());
    assert_eq!(
        attempt
            .results()
            .regions()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    assert_eq!(
        attempt.results().unprocessed_keys(&keys),
        vec![b"zz".to_vec()]
    );
}
#[test]
fn task_063_rejected_group_is_not_observed() {
    let mut c = BasicCluster::new();
    c.regions.set_region(region(1, "a", "z", 3));
    let ctl = ControllerAdmission::new(1);
    let keys = vec![b"m".to_vec()];
    let (mut attempt, submissions) = SplitAttempt::submit(&c, &ctl, [(1, group(&["m"]))].into());
    assert!(submissions.admitted.is_empty());
    assert_eq!(submissions.rejected.len(), 1);
    c.regions.set_region(region(2, "a", "m", 4));
    c.regions.set_region(region(3, "m", "z", 4));
    attempt.scan(&c.regions);
    assert!(attempt.finished()); // no accepted groups, not successful keys
    assert!(attempt.results().regions().is_empty());
    assert_eq!(attempt.results().unprocessed_keys(&keys), keys);
}
#[test]
fn task_063_binary_unbounded_and_id_overwrite() {
    let mut g = group(&["m"]);
    g.region = region(1, "", "", 2);
    g.keys = vec![vec![0, 255], b"t".to_vec()];
    let mut p = SplitGroupProgress::new(g);
    let mut r = RegionsInfo::new();
    let mut result = SplitKeyResults::default();
    let mut meta = region(3, "m", "", 3).meta().clone();
    meta.key_range.start_key = vec![0, 255];
    r.set_region(Arc::new(RegionInfo::new(
        meta.clone(),
        None,
        vec![],
        vec![],
    )));
    p.scan_regions_by_key_range(&r, &mut result);
    assert_eq!(result.regions()[&3], vec![0, 255]);
    assert_eq!(result.unprocessed_keys(&p.group.keys), vec![b"t".to_vec()]);
    meta.key_range.start_key = b"t".to_vec();
    meta.epoch.as_mut().unwrap().version = 4;
    r.set_region(Arc::new(RegionInfo::new(meta, None, vec![], vec![])));
    p.scan_regions_by_key_range(&r, &mut result);
    assert_eq!(result.regions()[&3], b"t");
    assert_eq!(result.unprocessed_keys(&p.group.keys), vec![vec![0, 255]]);
    assert!(!p.finished());
}

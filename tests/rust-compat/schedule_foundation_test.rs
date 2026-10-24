// Copyright 2026 AsterSQL.
use super::schedule_foundation::contract;
use crate::harness::fixture;
use astersql_pd_storage_kv::MemoryStore;
use std::sync::Arc;
#[tokio::test]
async fn schedule_foundation_memory() {
    contract(Arc::new(MemoryStore::default()), true).await;
}
#[tokio::test]
async fn schedule_foundation_etcd() {
    let mut f = fixture::Fixture::start();
    contract(Arc::new(f.store().await), true).await;
    f.command("stop", "stopped");
    f.command("start", "restarted");
    contract(Arc::new(f.store().await), false).await;
}

#[tokio::test]
async fn schedule_foundation_etcd_io_failure() {
    use astersql_pd_keypath::ClusterPaths;
    use astersql_pd_schedule_affinity::manager::{Manager, RestoreError};
    use astersql_pd_schedule_config::ProviderSnapshot;
    use astersql_pd_schedule_labeler::RegionLabeler;
    use astersql_pd_schedule_placement::{RuleManager, RuleManagerError};
    use astersql_pd_storage_endpoint::StorageEndpoint;
    let mut f = fixture::Fixture::start();
    let store = Arc::new(f.store().await);
    let paths = ClusterPaths::new(4199);
    let mut labels = RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
        .await
        .unwrap();
    let mut affinity = Manager::default();
    let endpoint = StorageEndpoint::new(store.clone());
    f.command("stop", "stopped");
    let failure = Manager::initialize(&endpoint, &paths, &labels, 100)
        .await
        .unwrap_err();
    assert!(matches!(failure, RestoreError::Storage(_)));
    assert!(
        RegionLabeler::load(StorageEndpoint::new(store.clone()), paths.clone(), 0)
            .await
            .is_err()
    );
    let mut placement = RuleManager::new(Arc::new(StorageEndpoint::new(store)), paths.clone());
    let config: ProviderSnapshot<String> = ProviderSnapshot::default();
    let failure = placement
        .initialize(3, &[], "", false, &config, &[])
        .await
        .unwrap_err();
    assert!(matches!(failure, RuleManagerError::Storage(_)));
    assert!(!placement.is_initialized());
    let failure = affinity
        .create_affinity_groups(
            &endpoint,
            &paths,
            &mut labels,
            &[astersql_pd_schedule_affinity::ranges::GroupKeyRanges {
                group_id: "failed".into(),
                key_ranges: vec![astersql_pd_keyutil::KeyRange::new(vec![1], vec![2])],
            }],
            100,
        )
        .await;
    assert!(failure.is_err());
    assert!(affinity.groups.is_empty());
    assert!(labels.get_all_label_rules(0).is_empty());
    f.command("start", "restarted");
    assert!(
        Manager::initialize(&endpoint, &paths, &labels, 100)
            .await
            .unwrap()
            .groups
            .is_empty()
    );
}

#[test]
fn schedule_foundation_dependency_direction() {
    use std::{
        collections::{BTreeMap, BTreeSet},
        process::Command,
    };
    // Inherit the caller's claimed Cargo slot; do not allocate an independent target.
    let target = std::env::var("CARGO_TARGET_DIR").expect("shared Cargo slot required");
    assert!(std::path::Path::new(&target).is_absolute());
    let output = Command::new("cargo")
        .args(["metadata", "--locked", "--format-version", "1", "--no-deps"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages: BTreeMap<_, _> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap(), p))
        .collect();
    let forbidden = [
        "statistics",
        "filter",
        "operator",
        "schedulers",
        "server",
        "runtime",
    ];
    for package in [
        "types",
        "keyrange",
        "rangelist",
        "plan",
        "config",
        "labeler",
        "placement",
        "affinity",
    ] {
        let root = format!("astersql-pd-schedule-{package}");
        let mut pending = vec![root.as_str()];
        let mut visited = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !visited.insert(name) {
                continue;
            }
            let Some(p) = packages.get(name) else {
                continue;
            };
            for dependency in p["dependencies"].as_array().unwrap() {
                if dependency["kind"] == "dev" {
                    continue;
                }
                let dependency = dependency["name"].as_str().unwrap();
                assert!(
                    !forbidden
                        .iter()
                        .any(|part| dependency == format!("astersql-pd-{part}")
                            || dependency == format!("astersql-pd-schedule-{part}")),
                    "{root}: forbidden {name} -> {dependency}"
                );
                if name == root {
                    assert_ne!(
                        dependency, "astersql-pd-proto",
                        "domain must not directly import protocol"
                    );
                }
                if packages.contains_key(dependency) {
                    pending.push(dependency);
                }
            }
        }
        println!("{root} normal workspace closure: {visited:?}");
    }
}

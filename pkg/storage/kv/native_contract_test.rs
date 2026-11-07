// Copyright 2026 AsterSQL.
use tempfile::TempDir;

#[path = "contract_test.rs"]
pub(super) mod contract;

use crate::native_open_test::standalone;

macro_rules! native_case {
    ($($name:ident => $case:path),* $(,)?) => {$ (
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() {
            let directory = TempDir::new().unwrap();
            let (network, store) = standalone(directory.path()).await;
            $case(&store).await;
            network.close(1, store).await;
        }
    )*};
}

native_case!(
    theme10_native_contract_atomic => contract::cas_atomic_revision,
    theme10_native_contract_scan => contract::snapshot_scan,
    theme10_native_contract_replay => contract::watch_replay,
    theme10_native_contract_compaction => contract::compaction_recovery,
    theme10_native_contract_expiry => contract::lease_expiry,
    theme10_native_contract_renew => contract::lease_renew_revoke,
    theme10_native_contract_fencing => contract::election_fencing,
    theme10_native_contract_invalid => contract::invalid_atomic,
    theme10_native_contract_cancel => contract::watch_cancel,
    theme10_native_contract_previous => contract::task_033_watch_previous,
    theme10_native_contract_boundary => contract::task_033_watch_boundary,
    theme10_native_contract_revision_txn => crate::revision_txn_test::revision_txn_contract,
    theme10_native_contract_raw_branches => crate::raw_branches_test::raw_branches_contract,
    theme10_native_contract_raw_get => crate::raw_get_test::raw_get_contract,
    theme10_native_contract_raw_range => crate::raw_range_test::raw_range_contract,
    theme10_native_contract_raw_compare => crate::raw_compare_test::raw_compare_contract,
);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn theme10_native_backup_is_one_revision_during_writes() {
    use crate::{MetaStore, Mutation};

    let directory = TempDir::new().unwrap();
    let (network, store) = standalone(directory.path()).await;
    let writer = async {
        for value in 0_u8..32 {
            store
                .transact(
                    vec![],
                    vec![Mutation::put(b"a", &[value]), Mutation::put(b"b", &[value])],
                )
                .await
                .unwrap();
        }
    };
    let archives = async {
        for _ in 0..12 {
            let bytes = store.backup().await.unwrap();
            let archive: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let revision = archive["image"]["revision"].as_i64().unwrap();
            let entries = archive["image"]["entries"].as_array().unwrap();
            if revision == 1 {
                assert!(entries.is_empty());
            } else {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0]["value"], entries[1]["value"]);
                assert!(
                    entries
                        .iter()
                        .all(|entry| entry["mod_revision"] == revision)
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    };
    tokio::join!(writer, archives);
    network.close(1, store).await;
}

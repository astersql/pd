// Copyright 2026 AsterSQL.
#[path = "contract_test.rs"]
mod contract;
macro_rules! contract_case {
    ($($name:ident),* $(,)?) => {$ (
        #[tokio::test]
        async fn $name() { contract::$name(&crate::MemoryStore::default()).await; }
    )*};
}
contract_case!(
    cas_atomic_revision,
    snapshot_scan,
    watch_replay,
    compaction_recovery,
    lease_expiry,
    lease_renew_revoke,
    election_fencing,
    invalid_atomic,
    watch_cancel
);

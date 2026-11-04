// Copyright 2026 AsterSQL.
mod recovery_start;
pub use recovery_start::*;
#[cfg(test)]
#[path = "recovery_start_test.rs"]
mod recovery_start_test;

#[cfg(test)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod etcd_fixture;

mod collect_report;
pub use collect_report::*;
#[cfg(test)]
#[path = "collect_report_test.rs"]
mod collect_report_test;

mod region_index;
pub use region_index::*;
#[cfg(test)]
#[path = "region_index_test.rs"]
mod region_index_test;

mod force_leader;
pub use force_leader::*;
#[cfg(test)]
#[path = "force_leader_test.rs"]
mod force_leader_test;

mod demote_failed;
#[cfg(test)]
#[path = "demote_failed_test.rs"]
mod demote_failed_test;

mod tiflash_cleanup;
#[cfg(test)]
#[path = "tiflash_cleanup_test.rs"]
mod tiflash_cleanup_test;

mod empty_region;
pub use empty_region::*;
#[cfg(test)]
#[path = "empty_region_test.rs"]
mod empty_region_test;

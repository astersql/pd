// Copyright 2026 AsterSQL.
//! Classic single-group timestamp oracle with durable physical windows.
mod oracle;
pub use oracle::*;
#[cfg(test)]
extern crate self as astersql_pd_tso;
#[cfg(test)]
mod oracle_test;
#[cfg(test)]
mod restart_test;

mod metrics;
pub use metrics::OracleMetrics;

mod tso_group_watch;
pub use tso_group_watch::*;
#[cfg(test)]
mod tso_group_watch_test;

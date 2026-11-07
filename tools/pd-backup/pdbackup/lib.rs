// Copyright 2026 AsterSQL.
//! Metadata fields consumed by offline PD recovery.

mod collect;
mod output;

pub use collect::{BackupInfo, collect};
pub use output::write_backup;

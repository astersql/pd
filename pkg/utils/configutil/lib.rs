// Copyright 2026 AsterSQL.
//! Pure configuration values and field presence, independent of parsers.
mod config_adjust;
pub use config_adjust::*;
mod config_metadata;
pub use config_metadata::ConfigMetaData;

#[cfg(test)]
mod config_adjust_test;

#[cfg(test)]
mod config_metadata_test;

mod config_unknown;

#[cfg(test)]
mod config_unknown_test;

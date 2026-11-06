// Copyright 2026 AsterSQL.
//! Standalone process assembly for the Rust lifecycle shell.
#[path = "main.rs"]
mod entry;
pub use entry::main;

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;

#[cfg(test)]
mod cli_config_test;

mod cli_config;

#[cfg(test)]
mod cli_exit_test;

mod cli_exit;

#[cfg(test)]
mod cli_logging_test;

mod cli_logging;

#[cfg(test)]
mod process_shutdown_test;

mod standalone_tso;

mod standalone_mcs;

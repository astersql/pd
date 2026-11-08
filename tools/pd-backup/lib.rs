// Copyright 2026 AsterSQL.
//! Process entry for the PD metadata backup tool.

mod entry;

pub use entry::run;

mod transport;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

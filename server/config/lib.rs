// Copyright 2026 AsterSQL.
//! Pure server configuration scalars. Full nested Config assembly is not implemented.
mod config_decode;
pub use config_decode::*;

#[cfg(test)]
mod config_decode_test;

mod config_urls;
pub use config_urls::*;

#[cfg(test)]
mod config_urls_test;

mod config_defaults;
pub use config_defaults::*;

#[cfg(test)]
mod config_defaults_test;

mod config_tso;

#[cfg(test)]
mod config_tso_test;

mod pd_server_defaults;
pub use pd_server_defaults::*;

#[cfg(test)]
mod pd_server_defaults_test;

mod pd_server_gc;

#[cfg(test)]
mod pd_server_gc_test;

mod pd_server_validate;
pub use pd_server_validate::*;

#[cfg(test)]
mod pd_server_validate_test;

mod keyspace_config;
pub use keyspace_config::*;

#[cfg(test)]
mod keyspace_config_test;

mod replication_config;
pub use replication_config::*;

#[cfg(test)]
mod replication_config_test;

mod microservice_config;
pub use microservice_config::*;

#[cfg(test)]
mod microservice_config_test;

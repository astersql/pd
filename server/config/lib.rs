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

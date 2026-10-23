// Copyright 2026 AsterSQL.

use std::{error::Error, fmt};

use astersql_pd_errs::{ErrorTemplate, TypedError, catalog::ERR_CONFIG_ITEM};

use crate::{ConfigUrlError, PDServerConfig, validate_url_with_scheme};

/// An invalid PD server value, preserving Go's plain, URL and coded errors.
#[derive(Debug)]
pub enum PDServerValidationError {
    /// GC concurrency is outside its valid range.
    GCConcurrency(&'static str),
    /// Dashboard address is not a valid request URL with scheme and host.
    Dashboard(ConfigUrlError),
    /// Key type is not table, raw or txn.
    KeyType(String),
    /// Negative flow rounding has the stable PD configuration error code.
    FlowRound(TypedError),
    /// A memory or GC tuning value is outside its valid range.
    Range(&'static str),
}

impl fmt::Display for PDServerValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GCConcurrency(message) | Self::Range(message) => f.write_str(message),
            Self::Dashboard(error) => error.fmt(f),
            Self::KeyType(key) => write!(f, "key-type {key} is invalid"),
            Self::FlowRound(error) => error.fmt(f),
        }
    }
}

impl Error for PDServerValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Dashboard(error) => Some(error),
            Self::FlowRound(error) => Some(error),
            _ => None,
        }
    }
}

impl PDServerConfig {
    /// Validates all Go PDServerConfig.Validate branches in source order.
    /// Apply defaults separately; this method never changes or clamps values.
    pub fn validate(&self) -> Result<(), PDServerValidationError> {
        use PDServerValidationError as E;
        self.validate_gc_concurrency().map_err(E::GCConcurrency)?;
        if !matches!(self.dashboard_address.as_str(), "auto" | "none") {
            validate_url_with_scheme(&self.dashboard_address).map_err(E::Dashboard)?;
        }
        if !matches!(self.key_type.as_str(), "table" | "raw" | "txn") {
            return Err(E::KeyType(self.key_type.clone()));
        }
        if self.flow_round_by_digit < 0 {
            // Go GenWithStack replaces the message while retaining the code.
            return Err(E::FlowRound(
                ErrorTemplate::new(
                    ERR_CONFIG_ITEM.code(),
                    "flow round by digit cannot be negative number",
                )
                .instantiate([] as [&str; 0]),
            ));
        }
        // Explicit comparisons preserve Go's acceptance of NaN. Range::contains
        // would reject NaN and change the contract.
        if self.server_memory_limit < 0.0 || self.server_memory_limit > 0.99 {
            return Err(E::Range("server-memory-limit should between 0 and 0.99"));
        }
        if self.server_memory_limit_gc_trigger < 0.5 || self.server_memory_limit_gc_trigger > 0.99 {
            return Err(E::Range(
                "server-memory-limit-gc-trigger should between 0.5 and 0.99",
            ));
        }
        if self.gc_tuner_threshold < 0.0 || self.gc_tuner_threshold > 0.9 {
            return Err(E::Range("gc-tuner-threshold should between 0 and 0.9"));
        }
        Ok(())
    }
}

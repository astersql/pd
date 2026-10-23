// Copyright 2026 AsterSQL.

use std::sync::Arc;

use crate::ConfigMetaData;

impl ConfigMetaData {
    /// Attaches the parser's ordered undecoded keys to this metadata view.
    ///
    /// Keys must already use the parser's display form (Go TOML Key.String),
    /// including quoting of literal dots and special characters. This pure
    /// value API neither parses TOML nor derives unknown keys from presence.
    pub fn with_undecoded_keys<I, S>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.undecoded = Arc::new(keys.into_iter().map(Into::into).collect());
        self
    }

    /// Returns a warning for all undecoded keys, including on child views.
    ///
    /// The caller decides where to collect the warning; unknown keys do not
    /// make otherwise valid configuration values fatal.
    pub fn check_undecoded(&self) -> Option<String> {
        if self.undecoded.is_empty() {
            None
        } else {
            Some(format!(
                "Config contains undefined item: {}",
                self.undecoded.join(", ")
            ))
        }
    }
}

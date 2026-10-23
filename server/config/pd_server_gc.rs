// Copyright 2026 AsterSQL.

use crate::PDServerConfig;

impl PDServerConfig {
    /// Validates only GC concurrency, without adjusting or changing any values.
    /// Call after applying defaults with the input's field-presence metadata.
    pub fn validate_gc_concurrency(&self) -> Result<(), &'static str> {
        if !(1..=16).contains(&self.gc_keyspace_index_load_concurrency) {
            return Err("gc-keyspace-index-load-concurrency must be between 1 and 16");
        }
        if !(1..=16).contains(&self.gc_state_cache_warmup_concurrency) {
            return Err("gc-state-cache-warmup-concurrency must be between 1 and 16");
        }
        Ok(())
    }
}

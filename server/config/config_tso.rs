// Copyright 2026 AsterSQL.

use crate::ConfigScalars;
use astersql_pd_configutil::{adjust_duration, adjust_int64};
use astersql_pd_typeutil::Duration;

impl ConfigScalars {
    /// Applies Config.Adjust's TSO defaults and physical interval bounds.
    /// Nonpositive durations use defaults before clamping. Negative proxy counts
    /// remain unlimited, and the save default is independent of leader_lease.
    /// This pure value projection does not log or adjust other configuration.
    pub fn adjust_tso_defaults(&mut self) {
        adjust_int64(&mut self.max_concurrent_tso_proxy_streamings, 5000);
        adjust_duration(
            &mut self.tso_proxy_recv_from_client_timeout,
            Duration::from_nanos(3_600_000_000_000),
        );
        adjust_duration(
            &mut self.tso_save_interval,
            Duration::from_nanos(5_000_000_000),
        );
        adjust_duration(
            &mut self.tso_update_physical_interval,
            Duration::from_nanos(50_000_000),
        );
        self.tso_update_physical_interval = self.tso_update_physical_interval.clamp(
            Duration::from_nanos(1_000_000),
            Duration::from_nanos(10_000_000_000),
        );
    }
}

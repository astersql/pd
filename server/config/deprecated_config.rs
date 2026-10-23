// Copyright 2026 AsterSQL.

use astersql_pd_configutil::ConfigMetaData;

use crate::ConfigScalars;

impl ConfigScalars {
    /// Applies only Config.Parse's compatibility block after a file was decoded.
    /// Pass root metadata and the caller's existing log destinations and warnings.
    /// CLI overrides, log defaults and file IO remain the caller's responsibility.
    pub fn adjust_deprecated_from_file(
        &self,
        config_file: &str,
        metadata: &ConfigMetaData,
        log_filename: &mut String,
        log_level: &mut String,
        warnings: &mut Vec<String>,
    ) {
        if !self.log_file_deprecated.is_empty() {
            warnings.push(format!(
                "log-file in {config_file} is deprecated, use [log.file] instead"
            ));
            if log_filename.is_empty() {
                log_filename.clone_from(&self.log_file_deprecated);
            }
        }
        if !self.log_level_deprecated.is_empty() {
            warnings.push(format!(
                "log-level in {config_file} is deprecated, use [log] instead"
            ));
            if log_level.is_empty() {
                log_level.clone_from(&self.log_level_deprecated);
            }
        }
        if metadata
            .child(["schedule"])
            .is_defined("disable-raft-learner")
        {
            warnings.push(format!(
                "disable-raft-learner in {config_file} is deprecated"
            ));
        }
    }
}

/// Checks Go's file migration rule with metadata rooted at pd-server.
/// Does not map trace-region-flow to a numeric rounding value.
pub fn migrate_configuration_from_file(metadata: &ConfigMetaData) -> Result<(), &'static str> {
    if metadata.is_defined("trace-region-flow") {
        return Err(
            "config item flow-round-by-digit and trace-region-flow(deprecated) are conflict",
        );
    }
    Ok(())
}

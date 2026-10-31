// Copyright 2026 AsterSQL.
//! Preserve the initiating startup error when releasing an acquired lease fails.
use crate::NodeError;

#[derive(Debug)]
struct CleanupError {
    startup: NodeError,
    cleanup: NodeError,
}
impl std::fmt::Display for CleanupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}; startup cleanup failed: {}",
            self.startup, self.cleanup
        )
    }
}
impl std::error::Error for CleanupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.startup.as_ref())
    }
}
pub(crate) fn preserve(startup: NodeError, cleanup: Result<(), NodeError>) -> NodeError {
    match cleanup {
        Ok(()) => startup,
        Err(cleanup) => Box::new(CleanupError { startup, cleanup }),
    }
}

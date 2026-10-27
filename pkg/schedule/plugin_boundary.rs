// Copyright 2026 AsterSQL.
//! Explicit boundary for the unsupported Go scheduler plugin ABI.
use crate::Coordinator;
use std::path::Path;

/// Requested operation, retained without storing or inspecting a plugin path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginAction {
    Load,
    Unload,
}
/// Go scheduler plugins cannot execute in the Rust scheduling graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Go scheduler plugins are not supported")]
pub struct UnsupportedGoPlugin {
    pub action: PluginAction,
}
impl<C> Coordinator<C> {
    /// Reject loading before any file access or scheduler registration.
    pub fn load_plugin(&self, _path: &Path) -> Result<(), UnsupportedGoPlugin> {
        Err(UnsupportedGoPlugin {
            action: PluginAction::Load,
        })
    }
    /// Reject unloading before any built-in scheduler can be removed.
    pub fn unload_plugin(&self, _path: &Path) -> Result<(), UnsupportedGoPlugin> {
        Err(UnsupportedGoPlugin {
            action: PluginAction::Unload,
        })
    }
}

// Copyright 2026 AsterSQL.
//! Store state predicates and legacy state transitions.
use crate::StoreInfo;

/// Legacy store state, retained independently of the node lifecycle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoreState {
    #[default]
    Up,
    Offline,
    Tombstone,
    Unknown(i32),
}

/// Node lifecycle, including unrecognized protocol values at adapter boundaries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeState {
    #[default]
    Preparing,
    Serving,
    Removing,
    Removed,
    Unknown(i32),
}

impl StoreInfo {
    pub fn state(&self) -> StoreState {
        self.meta().map_or(StoreState::default(), |m| m.state)
    }

    pub fn node_state(&self) -> NodeState {
        self.meta().map_or(NodeState::default(), |m| m.node_state)
    }

    pub fn is_up(&self) -> bool {
        self.is_serving() || self.is_preparing()
    }

    pub fn is_preparing(&self) -> bool {
        self.node_state() == NodeState::Preparing
    }

    pub fn is_serving(&self) -> bool {
        self.node_state() == NodeState::Serving
    }

    pub fn is_removing(&self) -> bool {
        self.node_state() == NodeState::Removing
    }

    pub fn is_removed(&self) -> bool {
        self.node_state() == NodeState::Removed
    }

    pub fn is_physically_destroyed(&self) -> bool {
        self.meta().is_some_and(|m| m.physically_destroyed)
    }

    /// Apply Go's SetStoreState mapping, preserving the physical flag except
    /// when explicitly setting Offline. Unknown legacy states are a no-op.
    ///
    /// # Panics
    /// Panics for absent metadata, or Offline without a physical destruction flag,
    /// matching the source option's programmer-error contract.
    pub fn set_store_state(&mut self, state: StoreState, physically_destroyed: Option<bool>) {
        let meta =
            std::sync::Arc::make_mut(self.meta.as_mut().expect("store metadata must be present"));
        match state {
            StoreState::Up => {
                meta.state = state;
                meta.node_state = NodeState::Serving;
            }
            StoreState::Offline => {
                let destroyed = physically_destroyed
                    .expect("physicallyDestroyed should be set when set store state to offline");
                meta.state = state;
                meta.node_state = NodeState::Removing;
                meta.physically_destroyed = destroyed;
            }
            StoreState::Tombstone => {
                meta.state = state;
                meta.node_state = NodeState::Removed;
            }
            StoreState::Unknown(_) => {}
        }
    }
}

// Copyright 2026 AsterSQL.
/// Persistent allocator scope. Keyspace IDs are cluster-wide, not per keyspace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllocatorNamespace {
    Global,
    Keyspace,
    KeyspaceNextGen,
}

impl AllocatorNamespace {
    pub(crate) fn key(self, cluster_id: u64) -> Vec<u8> {
        match self {
            Self::Global => format!("/pd/{cluster_id}/alloc_id"),
            Self::Keyspace | Self::KeyspaceNextGen => {
                format!("/pd/{cluster_id}/keyspaces/alloc_id")
            }
        }
        .into_bytes()
    }

    /// Last allocable ID, matching Go's explicit kernel mode boundaries.
    pub fn effective_end(self) -> u64 {
        match self {
            Self::Global => u64::MAX,
            Self::Keyspace => 0xFF_FFFF,
            Self::KeyspaceNextGen => 0xFF_FFFF - 1024 - 1,
        }
    }
}

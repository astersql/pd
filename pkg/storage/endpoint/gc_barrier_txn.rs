// Copyright 2026 AsterSQL.

use astersql_pd_storage_kv::Mutation;

use crate::{EndpointError, GCBarrier, GCStateWriteBatch, GlobalGCBarrier};

impl GCStateWriteBatch<'_> {
    /// Buffers a global barrier in this GC transaction without attaching a lease.
    pub fn set_global_gc_barrier(
        &mut self,
        barrier: &GlobalGCBarrier,
    ) -> Result<(), EndpointError> {
        let value = barrier.to_json()?;
        self.ops.push(
            Mutation::put(
                self.paths
                    .global_gc_barrier_path(&barrier.barrier_id)
                    .as_bytes(),
                &value,
            )
            .into(),
        );
        Ok(())
    }

    /// Buffers removal of a global barrier, including an absent ID.
    pub fn delete_global_gc_barrier(&mut self, id: &str) {
        self.ops.push(
            Mutation::Delete {
                key: self.paths.global_gc_barrier_path(id).into_bytes(),
            }
            .into(),
        );
    }

    /// Buffers the legacy service safe point representation of a scoped barrier.
    /// Expiration stays metadata; this adapter does not clean expired entries.
    pub fn set_gc_barrier(
        &mut self,
        keyspace_id: u32,
        barrier: &GCBarrier,
    ) -> Result<(), EndpointError> {
        let value = barrier.to_service_safe_point(keyspace_id).to_json()?;
        self.ops.push(
            Mutation::put(
                self.paths
                    .gc_barrier_path(keyspace_id, &barrier.barrier_id)
                    .as_bytes(),
                &value,
            )
            .into(),
        );
        Ok(())
    }

    /// Buffers removal of a scoped barrier, including an absent ID.
    pub fn delete_gc_barrier(&mut self, keyspace_id: u32, id: &str) {
        self.ops.push(
            Mutation::Delete {
                key: self.paths.gc_barrier_path(keyspace_id, id).into_bytes(),
            }
            .into(),
        );
    }
}

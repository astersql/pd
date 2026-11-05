// Copyright 2026 AsterSQL.

use crate::kv::Mutation;

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
        self.set_gc_barrier_with_service_id(keyspace_id, barrier.barrier_id.as_bytes(), barrier)
    }
    /// Keeps raw legacy service key bytes distinct from its JSON replacement representation.
    pub fn set_gc_barrier_with_service_id(
        &mut self,
        keyspace_id: u32,
        id: &[u8],
        barrier: &GCBarrier,
    ) -> Result<(), EndpointError> {
        let value = barrier
            .to_service_safe_point(keyspace_id)
            .to_json_with_raw_service_id(id)?;
        let mut key = self.paths.gc_barrier_prefix(keyspace_id).into_bytes();
        key.extend_from_slice(id);
        self.ops.push(Mutation::put(&key, &value).into());
        Ok(())
    }

    /// Buffers removal of a scoped barrier, including an absent ID.
    pub fn delete_gc_barrier(&mut self, keyspace_id: u32, id: &str) {
        self.delete_gc_barrier_with_service_id(keyspace_id, id.as_bytes());
    }
    /// Deletes the exact byte-valued legacy service ID.
    pub fn delete_gc_barrier_with_service_id(&mut self, keyspace_id: u32, id: &[u8]) {
        let mut key = self.paths.gc_barrier_prefix(keyspace_id).into_bytes();
        key.extend_from_slice(id);
        self.ops.push(Mutation::Delete { key }.into());
    }
}

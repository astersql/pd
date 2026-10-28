# Shared canonical region indexes

`RegionsInfo` is a cloneable handle to one `RegionState` behind an `Arc<RwLock<_>>`.
Cloning retains the same root, ID map and role indexes; it does not replay or copy
heartbeats. The original indexing algorithms remain in `regions*.rs` and
`region_stale.rs` on the private state. Each public call takes one read/write lock;
`atomic_check_and_put_region` validates and publishes root/subtree changes under
one write lock. No index guard crosses external async I/O.

`role_tree` returns a read guard, retaining the original region payload and
business-reference counts. Drop it before mutating the same registry. Iterator
and RNG callbacks must not reenter a write on this registry. Mutating APIs retain
exclusive mutable handle signatures; separate cloned handles serialize through
the same lock. Reset changes the shared generation for all handles.

`BasicCluster::clone` also shares `StoresInfo` membership/control state. Runtime
heartbeat pipelines keep their original state admission lock while Coordinator
and canonical policies receive handles to these same indexes. This removes the
previous need for an immutable, disconnected scheduling cache.

Evidence: runtime task 017 deterministic clock test updates a live heartbeat and
observes it through a retained cluster handle, receives the real policy command,
and verifies prepared store controls are released after shutdown. The existing
core/index and full workspace regressions cover the unchanged indexing behavior.

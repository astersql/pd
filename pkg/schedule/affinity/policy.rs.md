# Availability policy

`StoreCondition` preserves Go condition severity and strings. Its
`group_availability` and `affects_leader_only` methods map conditions to scheduling
policy. `collect_unavailable_stores` consumes real core `StoreInfo` values and a
borrowed `SharedConfigProvider`; first-match priority is removed/physical,
removing, unhealthy, leader eviction, busy, disconnected, low space, preparing.
This predicate order intentionally differs from enum severity. `is_slow` alone is
not an availability condition. `calc_group_availability` examines voters only and
ignores busy/leader eviction on followers.

`Manager::check_groups_availability` is a synchronous host-driven check. The host
passes nanoseconds for store health and seconds for group expiration, captured
from the same instant. An empty manager skips the check. `apply_store_availability`
compares the complete store snapshot, updates only changed group availability,
and never automatically restores an expired group. RuntimeGroupInfo's existing
strict `now > degraded_at + 600` boundary still applies when the snapshot does not
change. No timer, task, global state, protocol types or private runtime is added.

`update_group_availability`, `expire_affinity_group`, `degrade_affinity_group` and
`restore_affinity_group` reset the affinity count and advance the version for every
existing group, retaining region counts and persisted metadata. Unknown IDs are
no-ops. Explicit restore may revive an expired group. `check_has_unavailable_store`
returns canonical ErrAffinityGroupContent for the first matching voter in input
order; leader-only conditions on followers are skipped.

Availability and the private `Manager::unavailable_stores` snapshot are runtime
state and do not write MetaStore. Peer observation/version fencing belongs to
batch 37; the cancellable availability loop and metrics belong to batch 40.
Registered tests live in `affinity_availability_test.rs`; oracle and exact checks
are recorded in [batch 36 evidence](../../../docs/rust/schedule/affinity/36-证据.md).

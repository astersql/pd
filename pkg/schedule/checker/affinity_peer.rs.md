# Affinity peer and leader repair

`AffinityChecker::check` composes the canonical affinity `Manager`, persisted
`RegionLabeler`, placement `RuleManager`, `ReplicaStrategy`, statistics isolation
predicate and `PeerOperatorFactory::create_move_region`. `AffinityContext` borrows
host state and its region-tree existence result. Time, version/witness capability
and ID allocation come from the caller; no runtime or protobuf dependency is added.

The source is `affinity_checker.go` Check/createAffinityOperator and
isRegionPlacementRuleSatisfiedWithBestLocation. Go source and existing tests are
read-only contracts, **not executed oracle output**, under the user's waiver.
The non-merge path pauses before group queries, rejects unhealthy/unreplicated or
improvable placement, expires invalid target layouts, observes a valid region into
the real transactional manager, refetches its versioned cache, and clears membership
if the host reports a removed region. Already-affine regions produce no peer repair;
merge planning remains a separate task.

Virtual target fitting bypasses the placement cache. Its membership-only snapshot
preserves peer IDs/shared stores/learners and metadata; store replacement order is
ascending for deterministic non-witness cases. Go map order is unspecified; the
existing move-region factory accepts explicit role iteration order. This checker
uses ascending store order, without inventing runtime randomness. Full-region
statistics are unnecessary for this virtual placement/strategy gate.

Combo intent includes target voters and the exact existing learners. A voter/learner
store conflict aborts. Target leader must exist, allow transfer in and not match
reject-leader labels. Factory errors are returned as `BuilderError` diagnostics
while the operation remains absent (Go logs and returns nil). Observation storage
errors retain the manager's Go-compatible no-op behavior and expired/degraded state;
the next check retries. No partial operation or group publication is fabricated.

`record_op_success` takes the host's affinity schedule limit and Unix seconds,
records both related merge IDs for 60 seconds and prunes expired entries on writes.
`recently_merged` uses the same supplied clock; equality with expiry is still valid, as in Go TTL.Get. Hosts invoke recording on successful
completion only; this unit does not complete or execute operations itself.

`task_049_test.rs` explicitly registers frozen source-derived fixture assertions,
replays actual operator membership commands, checks group state and persisted
normalization, injects allocation/storage faults only at external boundaries, and
checks virtual placement and success-cache expiry. See
[task-049 evidence](../../../docs/rust/schedule-execution/task-049.md) for commands,
mutation evidence, provenance, and shared Cargo slot records.

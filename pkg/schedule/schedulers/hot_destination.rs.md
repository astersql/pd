# Hot destination selection

[hot_destination.rs](hot_destination.rs) ports the destination-side predicates of
Go `hot_region_solver.go`: `filterDstStores`, `pickDstStores`,
`destinationStoreFailure`, destination priority/history tolerance checks,
`isRevertRegionValid`, and reverse-region membership eligibility.

`HotDestinationSelector` takes the existing `HotRegionParams`, validated distinct
priority dimensions, RW type and `HotSourceOperation`. `destination_store_decision`
uses the canonical current/future **maximum**, strict `load * ratio < expectation`,
and every current history sample in each selected dimension. V1 strict mode uses
all selected priorities, ordinary V1 uses the first, V2 uses any; write leader
always uses the first. TiFlash adds 0.1 to the configured destination tolerance.
An invalid priority returns `HotDestinationError`; malformed history alignment
panics at the accessed sample as Go does, with a caught-panic regression test.

`HotDestinationScope` borrows the host's resolved store IDs and expectation.
`None` means global/legacy expectation (also the placement-v2 fallback). `Some`
checks scope membership before touching histories and thresholds. Population
construction remains with the subsequent solver; no placement index is copied.
`HotDestinationDecision` exposes exact load diagnostic labels for host metrics.
Engine admission is performed by the picker, separately from load diagnostics.

`filter_dst_stores` receives the real region through `PlacementGuardContext`,
current source/peer, optional canonical fit and optional load scope. Callers first
use the source selector to validate region health, replication and stale stats.
Store load details must be unique by store ID, corresponding to Go's map.
The returned borrowed details have no ranking order contract.

- Move-peer excludes all existing region stores and rejects read leader peers.
  It runs high-priority store state, hot eviction, special use and peer safeguard.
- Read leader actions require a leader source and exclude its store. Existing
  peers need leader state checks; new stores also need move-region and hot
  eviction checks. Placement leader safeguards allow moving a leader.
- Write leader actions use existing voter followers only and forbid moving a
  leader outside the region through the real placement guard.
- Non-TiKV engines support write move-peer only, require the config flag, and
  exclude TiFlash compute. Known reserved capacity is rejected; hotRegion is allowed.

`pick_dst_stores` applies borrowed canonical async filters in order before load
admission. It does not reserve tokens or mutate scheduling state. The assembled
path reuses existing store, filter and placement contracts; disabled placement
rules fall back to location diversity for peer moves and omit leader guards.
No private runtime, I/O, global subscribers or metrics are installed.

`allow_revert_region` checks absent original-source peer for peer moves and
present peer for leader transfers. `is_revert_region_valid` accepts the reverse
region context and its own fit, reversing source/target explicitly. The solver
still owns distinct-region identity, reverse-region health/staleness, and reverse
scope load checks. Witness restrictions and source-rule constraints are enforced
by the canonical guard, including read/write leader differences.

Evidence uses the user's **Go source/test derivation exemption**, not a Go
execution differential. Real Rust load/core/store filters and rules are exercised;
MemoryStore backs real RuleManager initialization and rule persistence. This
selector introduces no persistence operation or I/O fault point. See
[task-090](../../../docs/rust/schedule-execution/task-090.md),
[frozen inputs](../../../docs/rust/schedule-execution/fixtures/task_090.json) and
[tests](task_090_test.rs) for red/green, boundary and fault evidence.

# Hot placement load populations

`HotPlacementLoads` owns one immutable load snapshot per solver round. `scope`
ORs active rules' label constraints within one engine, caches by the Go
length-prefixed constraint value key (`PlacementScopeKey`), and returns local
`StoreLoadSummary` expectation/history/hot-peer count/normalized deviation only
when the matched population is nonempty and narrower than the engine population.
Empty, unmatched and unrestricted scopes return `None` (global fallback), including
cached misses. Rule identity, range and version are deliberately absent from the
key: RuleManager already selects rules for the region's range.

`prepare_for_region` uses the canonical region peer and `RegionFit`, rejecting
missing/witness source fits. Peer moves use the source rule; leader transfers
union all non-witness fits with the source placement role. Stale source labels
fall back to global statistics. The caller supplies a current fit and source
from the same scheduling round, and gates this path using placement/rank config.
This module does not create operators or install global metrics.

The index derives directly from sorted snapshot store IDs, with one bit per store;
there is no independently stale external index. Duplicate IDs return typed
`HotPlacementError::DuplicateStore`. Create a new `HotPlacementLoads` for each
changed load/label snapshot, dropping the old cache. Immutable Arc scopes can
outlive their round; population equality compares sorted IDs and engine to avoid
cross-snapshot bit-position aliasing. `population_words` exposes only round-local
encoding. `destination_scope` connects directly to task 90 admission and its
`dst-store-out-of-scope` diagnostic, before threshold/history indexing.

Sources: `hot_region_solver.go:954–1176`, related placement scope tests and
`hot_region.go:369–402`. Evidence is explicitly source-derived, under the user's
oracle exemption; no Go runtime output or Go/Rust runtime differential is claimed.
Tests use canonical StoreLoadDetail, RuleManager, RegionFit, MemoryStore and
HotDestinationSelector. This layer performs no I/O itself; stale/missing snapshots,
invalid duplicate IDs and malformed excluded-target history are its relevant
fault cases. See [task evidence](../../../docs/rust/schedule-execution/task-091.md).

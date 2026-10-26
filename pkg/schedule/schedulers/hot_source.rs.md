# Hot source selection

`HotSourceSelector` implements the source-side behavior of Go
`hot_region_solver.go:filterSrcStores`, `sourceStoreFailure`, `expectLoad`,
`filterHotPeers`, `sortHotPeers`, `isRegionAvailable`, `getRegion` and
`calcMaxZombieDur`. The canonical implementation is [hot_source.rs](hot_source.rs).

It borrows real `StoreLoadDetail`, `StoreLoadPred`, `HotPeerStat` and `RegionInfo`.
The constructor takes validated priorities, the existing `HotRegionParams`, RW type
and a typed action. Rank v1 uses the first priority, or both with strict picking;
write leader always uses the first. Rank v2 accepts either selected dimension.
Current/future minimum and every history sample use strict `>` thresholds.
TiFlash adds 0.1 tolerance and supports write move-peer only; compute is excluded.

`filter_src_stores` returns each borrowed detail and `HotSourceDecision` so the host
can publish diagnostics and retain only `admitted()` entries. With placement v2,
engine-specific restriction discovery can defer global safeguards. The host must
then supply the real scoped expectation to `source_store_decision`. Population
construction and target selection belong to the subsequent solver; no duplicate
placement index, store state, configuration or operator state is created here.
`expect_load` borrows the scope expectation, falling back to the detail expectation.

Peer selection computes nth peers before pending/cooldown exclusion. Above the cap,
it alternates unique pointer identities from two descending priority lists and
then excludes pending and cooling peers, without refill. Its returned union order
is unspecified, like the Go map; equal-rate Go sort ties are also unspecified.
The normal nth position is 10; an explicit position supports deterministic inputs.
Zero disables nth extraction; `None` means this input has no nth peer. The owner
must refresh its per-round nth view rather than retain stale entries. Clock input
is explicit nanoseconds; cooldown delegates to canonical `HotPeerStat`.

`get_region` allows pending replicas, rejects down replicas and checks either exact
legacy replication or the actual canonical `RuleManager::fit_region`. It returns
the successful `Arc<RegionFit>` for later placement safeguards. Missing region,
unhealthy replication, abnormal count/rules and stale source peer/leader produce
`HotSourceError`; errors cannot carry an old fit. Region lookup remains with the
host. Invalid priorities/negative peer limits fail construction. Malformed history
alignment remains a programmer precondition and panics at the accessed sample,
as Go does; it is covered by an explicit caught-panic fault test.

No I/O, private runtime or global metrics are installed. The real in-memory
MetaStore backs placement rule initialization in tests; this task introduces no
persistence boundary requiring additional fault injection. Zombie duration reuses
the config's store/region statistic duration APIs according to source provenance.

Evidence is **Go source/test derivation**, under the user's explicit exemption;
no Go runtime output or Go/Rust execution differential is claimed. See
[task-089](../../../docs/rust/schedule-execution/task-089.md),
[frozen fixture](../../../docs/rust/schedule-execution/fixtures/task_089.json) and
[tests](task_089_test.rs) for actual red/green and fault verification.

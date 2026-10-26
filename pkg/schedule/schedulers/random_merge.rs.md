# Random merge scheduling

`RandomMergeScheduler` selects one eligible source store, samples one healthy leader
region inside `RandomMergeConfig` ranges, and selects the next adjacent region unless
bidirectional parity selects the previous one or the next region is absent. One-way
mode never falls back. A rejected chosen target does not trigger another attempt.
Target ranges need not lie inside configured source ranges. No merge-size or recent
split cooldown gate is imported from the automatic merge checker.

The library borrows the host's canonical BasicCluster, RuleManager, optional
RegionLabeler and StatisticsSnapshot. `RandomMergeContext` exposes time, random
index/shuffle, parity and protocol capability at host boundaries. Go map order is
unspecified; sorted store IDs stabilize only that order. Index callbacks must return
an index below the supplied nonzero length, following the existing core contract.

Health and replica satisfaction use the filter crate; heat uses statistics membership
and the host's degree threshold. Shared `checker::allow_merge` enforces adjacency,
rule/label split boundaries, merge deny labels and table policy. No peer-store gate
specific to automatic merge checking is added. `RangeOperatorFactory::create_merge`
constructs the active/passive pair, aligns peer membership when needed, and propagates
typed BuilderError without returning a partial pair. Both operators have LOW priority.
Admission is the strict merge count < limit check, separately invoked by the host.

`new`, `config`, `encode_config`, `is_schedule_allowed` and `schedule` form the domain
API. Configuration preserves null/empty ranges, lowercase hex key JSON and existing
CLI decoding. No reload API is invented for Go's outstanding TODO. HTTP, registry,
metrics publication and submitting operators belong to the host/adapters. Configuration
adds no persistence path. Placement/label tests use canonical MemoryStore endpoints.

[Task evidence](../../../docs/rust/schedule-execution/task-098.md) records the explicit
user authorization for source-derived expectations rather than Go runtime differential
results. Sources are `random_merge.go`, `random_merge_test.go`, `checker/merge_checker.go`,
`filter/filters.go`, `operator/create_operator.go`, and `utils/keyutil/keyrange.go`.
Tests live separately in `task_098_test.rs`, explicitly registered in `lib.rs`.

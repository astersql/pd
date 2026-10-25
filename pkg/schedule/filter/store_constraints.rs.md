# Store capacity and static constraints

`store_constraints.rs` extends the canonical `astersql-pd-schedule-filter` crate
through the existing synchronous `Filter<V>` API. The six filters borrow the host's
`SharedConfigProvider` and real `StoreInfo`. They return complete plan statuses;
they do not mutate stores, reserve admission budgets, access persistence, or own a
runtime. `FilterType` values retain Go's identities 1/3/6/7/8/9.

- `StorageThresholdFilter` admits every source, and rejects a target using
  `StoreInfo::is_low_space`. Its zero capacity, strict ratio, 8 GiB and 100-region
  exemptions remain core responsibilities.
- `LabelConstraintFilter` uses placement's conjunction and exclusive-label gate.
  `EngineFilter` uses only its single `LabelConstraint`, as Go does; missing engines
  pass `not_special_engines()` and fail `special_engines()`.
- `SpecialUseFilter` subtracts allowed values from `hotRegion` and `reserved`.
  Unknown uses are admitted. Low-space sources can evacuate even when their special
  use is disallowed; targets receive no capacity exemption from this filter.
- `IsolationFilter` owns a constructor-time snapshot of each region store's location
  prefix. An unknown isolation level falls back to index zero; empty region stores
  reject targets. Missing values match missing values. Matching values use Unicode
  simple folding; the existing core `equal_fold` helper is now public so this path
  shares core's implementation rather than duplicating it. Empty location labels
  with nonempty region stores panic, preserving the Go programming-error boundary.
- `HotRegionEvictedTargetFilter` rejects any slow, stopping or slow-trend eviction
  reference, while admitting sources.

The placement dependency is the existing canonical crate, with no new registry
dependency or server/runtime edge. Its existing rule manager uses storage-endpoint,
which transitively depends on proto for separate keyspace persistence adapters.
This filter source and its public API use only domain types; this task neither
introduces a direct protocol dependency nor restructures that existing crate chain. Status errors are typed `StatusCode` values with
unchanged empty detailed reasons; malformed label operators reject through the
placement contract. No external I/O boundary exists to inject a persistence fault.

[Task evidence](../../../docs/rust/schedule-execution/task-008.md) records actual Go
returns, provenance, normalization, red tests, six rejected production mutations,
constructor faults and workspace checks. The independent
[tests](task_008_test.rs) replay frozen outputs against real domain objects and
verify ordered selection, handle identity, constructor boundaries and faults.

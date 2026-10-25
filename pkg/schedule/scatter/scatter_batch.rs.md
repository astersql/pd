# Admin batch scatter

`ScatterBatch::scatter` composes the canonical `RegionsInfo` range/ID lookup,
`is_region_replicated`, optional host `AffinityFilter`, `ScatterTargetSelector`,
`create_scatter_operator` and `ControllerAdmission::add_limited_operators`.
It borrows host state, runs serially on the host runtime/LocalSet, and never owns
an independent state registry or runtime. The allocator is the external I/O
boundary. The host supplies its current hot-region set, clock and affinity filter;
`None` means affinity is disabled. The host also consumes admitted controller
state for heartbeat dispatch and removed/terminal operator archival, as with
other canonical admission users. This module does not implement a server loop.

The source is `region_scatterer.go:480–590,604–676,825–876`; provenance and tests
are recorded in [task-061](../../../docs/rust/schedule-execution/task-061.md).
Evidence is read-only Go source/test derivation and executed Rust behavior tests,
not Go runtime output or a runtime differential. Task 60 supplies operation
construction and accounting. Rules come from the same host `RuleManager` used
for replication fitting, including default rules in legacy placement mode.
The manager must be initialized by the host before scattering.

IDs are looked up before deduplication; each missing occurrence increments the
Go-compatible no-region counter. Empty requests/ranges and multi-ID requests with
no live region return `EmptyRegion`; one missing ID returns `RegionNotFound`.
Missing IDs in mixed requests remain per-region failures. Snapshots remain fixed
through retries, as Go's region map does. Region/store iteration uses ascending
IDs to make Go's otherwise unspecified map/tie ordering explicit. This selects
one valid deterministic order and does not claim identical random target choice.

`processed` retains Go's opsCount definition: successful Scatter calls, including
no-op and controller rejection. Admission failures stay in `failures` but are
removed from `pending` and never retried. Builder/selection/gate errors remain
pending; later success clears their earlier failure. Factory accounting happens
before admission and is not rolled back on controller rejection. A negative
retry limit performs no attempt. The upper clamp is 30 (31 total attempts).
Backoff is 100ms × 2^round, capped at 60s, including the final failed round's
sleep. A region's pending peers alone do not imply replication failure.
Unreplicated IDs are returned as `suspect` so the host can call its existing
`addSuspectRegions(false, ids...)`; this is distinct from retry-pending IDs.

Cancellation is an explicit Rust entrypoint enhancement requested by task 61:
Go's current batch loop has no context check. A `CancellationToken` stops before
the next region or interrupts backoff; completed admission/accounting is retained.
Latest typed failures are preserved, and never-attempted regions receive
`Cancelled`. A single in-flight region operation completes as a unit; dropping
that future is not a supported transactional rollback mechanism.

`ScatterBatchMetrics` caches the bounded Go type/event vocabulary in a host-owned
Prometheus Registry. It preserves `pd_schedule_scatter_operators_count` with
`type,event`; registration errors propagate without global initialization.
No ID, group or dynamic store labels are added; this module does not own the
separate distribution metric. `success` means operator construction, even if
admission later rejects it; `unnecessary` counts only actual no-op layouts.
The public outcome supplies cancellation/pending state without inventing metric
labels. Typed selector, builder and admission errors expose their sources.

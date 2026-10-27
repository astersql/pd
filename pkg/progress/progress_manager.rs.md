# Store progress manager

`Manager` is the canonical `pkg/progress` manager, re-exported by `lib.rs`.
It consumes real `astersql_pd_core::StoreInfo` and reuses `ProgressIndicator`;
no protobuf, persistence backend, private runtime, or global registry is installed.
`new` accepts a short, non-reentrant `Arc<dyn Fn() -> Duration + Send + Sync>`, a
positive update interval and host `prometheus::Registry`. Invalid intervals and
registration failures return `prometheus::Error`; partial registration rolls back.

`update_progress` / `update_progress_at` serialize transitions with queries,
`reset`, getter replacement and GC. Queries return independent values. Preparing
targets only grow. Removing fixes its target at the first sample, converts the
remaining size to nonnegative cumulative work, and calls the current patrol
getter before each later sample. Leaving an action pushes its target, moves it
to the completed map, then optionally starts the next action. Natural numerical
completion alone does not remove an active entry. Absent stores and unrelated
node states are ignored. Average ETA sums that overflow saturate to `f64::MAX`.

The three GaugeVec names, help strings and labels match `metrics.go`:
`pd_cluster_progress`, `pd_cluster_speed`, `pd_cluster_eta`, labels
`address=""`, decimal `store`, `action`. Initial insertion creates no series.
Later samples cache all three gauge handles per store/action; GC removes the
series and cache. Completion deliberately leaves the last sampled metrics
unchanged, including when the same store is active again. Reset clears active
entries, series and handles but retains completed entries, matching Go. GC of
an entry without series is safe, including after reset. GC deletes only the
completed action's label set, even if a newer active entry shares those labels,
as in the source implementation.

`gc_completed_progress_at` uses strict age > 30 seconds; equal TTL and backwards
clock observations retain entries. `gc` waits 120 seconds before its first tick,
uses the caller's Tokio runtime, prioritizes cancellation and returns for the
caller to join. It creates no detached task; the caller owns any spawn and must
cancel/join it. Completion timestamps use SystemTime; deterministic tests inject
epoch timestamps. Background tests pause Tokio's timer and use already-old
completion timestamps, without pretending wall time advanced with Tokio.

Source: `progress.go`, `metrics.go`, `progress_test.go`; expectations are explicitly
source-derived under user authorization. The archived admission native GC trace
supports metric retention/deletion and join only; it is not a fresh Go execution
or a complete manager differential. See
[task-132 evidence](../../docs/rust/schedule-execution/task-132.md) and its frozen
fixture, red assertion, real behavior and registration-failure tests.

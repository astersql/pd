# Region overlap statistics cleanup

[region_overlap.rs](region_overlap.rs) exposes
`handle_region_heartbeat_with_overlap_cleanup` as an opt-in composition over the
canonical heartbeat entry. It commits the core root and role indexes first, then
clears every evicted ID from optional `RegionStatistics` and marks it in
`LabelStatistics`. The host serializes core/statistics mutation for this call.
Same-ID replacements are excluded from the overlap list, so a shrinking or merged
survivor keeps its own classification until the host observes its latest report.
Stale reports return before either statistics index is touched.

Labels retain their counts until the existing `clear_defunct_regions` collection
boundary. Repeated reports and repeated collection do not double-decrement them.
The function returns the unchanged heartbeat result for further host side effects.
It does not observe incoming classification or isolation: those require host-owned
configuration, placement and store snapshots, as in Go's later Collect task.

Go `pkg/cluster/cluster.go::HandleOverlaps` does not remove hot peers. This entry
therefore preserves hot history until the existing peer expiration, cold report
or TTL path runs. It does not synthesize a same-ID replica report from an evicted
range, which would destroy history used for inheritance. Task22 provides the
existing Go TTL/cold oracle; task44 tests both directions across an overlap and
an explicit host-clock deadline. Placement/affinity invalidation, persistence,
sync and scheduling remain their owners' responsibilities. No private runtime,
protocol dependency, backend or background task is introduced.

The independent [tests](region_overlap_test.rs) are explicitly registered in
[lib.rs](lib.rs). Evidence and limitations are in
[task-44.md](../../docs/rust/state/task-44.md).

Task 128 extracts the anomaly/label mutations into
`astersql_pd_cluster_utils::clear_overlap_statistics`, shared with the scheduling
bridge. This opt-in state-only API retains its existing statistics-only signature;
the full heartbeat effects path uses `handle_overlaps` to additionally invalidate
placement and the host-installed affinity manager under its state lock.

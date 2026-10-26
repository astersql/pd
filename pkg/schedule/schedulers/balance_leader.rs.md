# Leader balancing

`BalanceLeaderScheduler::schedule` is the canonical asynchronous domain entrypoint.
It borrows `LeaderBalanceContext`: canonical `BasicCluster`, one scheduler config
snapshot, mutable placement cache, real affinity/label and statistics snapshots,
key-range manager and pending `OpInfluence`. The caller supplies random-index,
shuffle and wall-clock observations. No runtime, transport, background worker or
private cluster state is created. Host coordination supplies current operator
count and configured limit to `is_schedule_allowed`; schedule itself does not
reserve admission capacity, matching Go's separate admission hook.

The earlier synchronous `Scheduler` trait cannot await real store budgets and
`PeerOperatorFactory`. This implementation exposes an async inherent method;
it does not block a runtime or alter the shared base/registry contract. Runtime
facade/factory integration remains with its owning later task. This task exports
the concrete scheduler and tests from the existing canonical crate.

`BalanceLeaderConfig` validates batch 1..=100, defaults to 4, deep-clones ranges,
and supports the existing typed `SchedulerConfig` persistence boundary through
Serde. Range JSON uses the canonical keyutil representation. HTTP routes and
metrics registry wiring remain adapter responsibilities.

Candidates order by pending-adjusted leader score, preferring larger store IDs
within Go's absolute 1e-6 tie tolerance. Binary search saves both changed-store
positions before applying the operator's total influence, then incremental resort
follows Go's `pos > 1` upward boundary. Source and target attempts alternate;
retry quotas attenuate on failure, reset on success and GC departed stores.
Accepted regions are unique within the batch. Pending influence is copied once,
so scheduling cannot mutate the controller's input generation.

The scheduler applies canonical transfer-leader store predicates, special-use,
pending/down, affinity, hot-region and placement-leader guards. The existing
`BalanceSolver` applies conservative pending influence and count/size tolerance.
Only a strictly improving projected score admits a real `PeerOperatorFactory`
transfer. Factory errors remain typed in `LeaderBalanceResult.errors`; dry-run
plans retain filter/score/factory rejection stages. Accepted operators carry
sourceScore/targetScore metadata and require store health checks.

As in the Go source, only the default scheduler subtracts registered key ranges,
and only from its first configured range. Empty sampling ranges retain canonical
core's whole-keyspace meaning. The scheduler does not silently introduce new
range semantics. Follower-score ties have no Go observable stable order; tests
accept any eligible tied follower while candidate ordering is tested exactly.

The source contract is `balance_leader.go`, its adjacent tests, `utils.go` and
canonical operator/filter implementations. Evidence is **read-only Go source
inference plus Rust execution**, under the user's explicit waiver of Go-runtime
oracle admission. No Go runtime output or differential result is claimed.

See [task-070 evidence](../../../docs/rust/schedule-execution/task-070.md) for the
31 frozen scenarios, candidate/config tests, negative mutation, real transfer
application, commands, source hashes and shared Cargo slot records.

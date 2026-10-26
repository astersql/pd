# Witness leader heartbeat recovery

`TransferWitnessLeaderScheduler` owns a bounded 10,000-entry channel of immutable
`Arc<RegionInfo>` snapshots. `recv_region_info` clones its producer; blocking `send`
and nonblocking `try_send` provide backpressure. The host must use `try_send` or a
blocking producer outside its async executor when a full queue could block progress.
The host determines heartbeat eligibility, supplies the runtime, clock, random draw,
canonical cluster/config/rules, and submits the returned operators. No private runtime,
protocol types, second cluster cache or persistence is introduced.

`is_schedule_allowed` always returns true; `schedule` consumes at most three events,
including duplicates and rejected events. It never deduplicates, retries or requeues.
Current store health/budgets are checked when consuming a queued region snapshot.
Pending/down peers are excluded by store ID. Follower membership comes from canonical
`RegionInfo::followers`; absent stores are omitted. `StoreStateFilter` checks transfer-in
safety at Urgent priority. The canonical `PeerOperatorFactory` validates roles, placement
and builder safety, builds a single multi-target transfer, and adds LEADER to
WITNESS_LEADER. The operator priority is Urgent. Peer allocation must never occur.

Read-only Go sources: `transfer_witness_leader.go`, its adjacent two tests,
`core/region.go`, `core/store.go`, and the canonical operator builder/factory. Go map
order is unspecified; Rust sorts follower store IDs before the host's random index.
Candidate order and selected index are explicit fixture normalization. Go includes
witness/demoting followers in the initial candidate set: selecting one propagates
builder rejection; this scheduler does not invent an additional retry policy or role
filter. Ordinary leader events are accepted too, as in Go's existing test: heartbeat
eligibility is a coordinator responsibility. Alternative IDs retain Go builder semantics.

`TransferWitnessLeaderError` distinguishes NoTarget from typed Builder failures. Go
logs these failures and consumes the event; the Rust result exposes them for host
observability while continuing the batch. There is no mutable configuration or saved
queue in the Go scheduler, so storage failure injection is inapplicable here.

[Task 103 evidence](../../../docs/rust/schedule-execution/task-103.md) records the
user-authorized source/test-derived fixture (not a Go runtime oracle), behavioral
mutation failure, real operator/heartbeat acknowledgement checks, queue and budget
faults, and full workspace verification. Tests live separately in `task_103_test.rs`.

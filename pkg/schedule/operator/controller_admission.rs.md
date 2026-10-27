# Controller admission and removal

[controller_admission.rs](controller_admission.rs) implements the admission container
of `astersql-pd-schedule-operator`, on the canonical `Operator`, `BasicCluster`,
`RandBuckets` and `WaitingOperatorStatus`. It does not own a second region registry.
The host supplies a stable cluster read snapshot; normal Rust borrowing prevents
mutations of that snapshot during admission. No runtime or persistence is installed.

`ControllerAdmission::add_operators` checks all supplied regions, exact version and
confver equality (absent epochs use Go's zero-valued getters), strictly higher
priority, CREATED status, waiting description quota and finally expiration. The
expiry pass evaluates every member, preserving the actual EXPIRED status transition.
Rejected source cases preserve their `CancelReason` and do not change running counts.
`operator_count` looks up the exact primary scheduler kind, not every step kind bit.

One container mutex serializes admission, replacement, removal and snapshots. New
members are checked and started before map mutation. Existing members being replaced
are detached before their remaining partners, and new members are inserted last.
This realizes this task's atomic pair requirement and prevents partner cleanup from
removing a newly installed replacement. Old replaced members become REPLACED; a
remaining old partner becomes CANCELED with `RelatedMergeRegion`.

`remove_operator` compares Arc identity, so a stale dispatch reference cannot remove
its replacement. It removes the related member synchronously. `remove_operators`
drains running state and counts; iteration order is unspecified. Terminal operators
are returned to the host for later history/limiter integration. Transport dispatch,
store-limit checks/acknowledgements, notifier scheduling and retained terminal history
are outside this task. This API is a controller component, not a full replacement
for Go Controller's AddOperator entry point.

`enqueue_waiting` inserts one operator or a reciprocal pair and counts a pair once
using its first description. The host calls `promote_waiting` after enqueue to compose
Go AddWaitingOperator's immediate promotion. Promotion bypasses waiting quota,
revalidates region state, decrements queue counts on both success and rejection, and
skips rejected batches until admission or exhaustion. Random sampling is a host
boundary; a supplied sample is reused on retries and each dequeue renormalizes
nonempty bucket weights. Invalid queue inputs return typed errors without consuming
the queue. A complete host may supply a fresh sample on successive promotion calls.

Malformed batches (empty, duplicate region, missing/nonreciprocal partner) return
`InvalidBatch` before running mutation. Source-defined rejection returns
`Rejected(CancelReason)`; invalid priority/sample retains `QueueError`. The host must
not externally mutate submitted operators' status, priority or pairing metadata
while the container is using them; clock callbacks must not reenter the container.

The read-only Go sources are `operator_controller.go` and
`operator_controller_test.go`, frozen to commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`. Expectations are source-derived under the
user's explicit waiver of Go execution admission. No Go/Rust execution differential
is claimed. The fixture records source hashes, inputs and outcomes, while the
[evidence](../../../docs/rust/schedule-execution/task-034.md) records actual Rust
behavior, mutation failure, tests, Cargo slot and read-only verification.

Task 118 adds `waiting_operators`, a non-consuming snapshot under the admission
mutex, forwarding the canonical RandBuckets list (including merge partners).
It neither dequeues nor changes waiting counts; used by the management handler.

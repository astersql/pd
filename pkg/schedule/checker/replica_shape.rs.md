# Replica count and location repairs

`ReplicaShapeChecker::check(ReplicaShapeContext, region, allocate)` implements the
normal-cluster tail of Go ReplicaChecker.Check. The host invokes it after fault
repair (task 41) produces no operator, and supplies the coherent store/config/rule
snapshot and version capability. Its independent pause controller allows the host
to apply the same pause deadline to both stages.

The context and outcome reuse task 41's canonical snapshot and diagnostic types.
No retry cache, store statistics, rule state, Tokio runtime or storage backend is
copied into the checker. `pending` requests insertion into the host's retry cache;
it never requests removal. Builder errors remain typed and do not prevent later
shape stages from being attempted.

Make-up counts all peers, including learners; extra removal counts voters. Add
runs first, extra removal second, then strictly improving location replacement.
The real ReplicaStrategy handles eligibility, storage, isolation and diversity;
PeerOperatorFactory handles allocation, placement, leader transfers and steps.
Make-up uses HIGH priority, other shape repairs retain the operator default.
An unavailable removal source always sets pending; an add target only does so for
temporary rejection; location improvement deliberately ignores that flag.

The Go source and adjacent tests are read-only contracts. Per user authorization,
there is no Go execution prerequisite: task 42 records source-derived expectations
and Rust behavior evidence, without claiming runtime Go/Rust differential results.
See [task-042](../../../docs/rust/schedule-execution/task-042.md) for provenance,
fixed fixtures, error injection, heartbeat application and validation commands.
This checker has no persistence I/O; allocator failure is the external fault
boundary. It produces intents without mutating region/store snapshots.

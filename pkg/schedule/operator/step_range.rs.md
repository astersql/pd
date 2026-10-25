# Range step domain contracts

`MergeRegion` and `SplitRegion` use canonical core `RegionInfo`/`RegionMeta`,
`OpInfluence`, and hbstream owned `Operation`. They do not access storage,
transports, clocks, or a private runtime. Commands are snapshots for the existing
heartbeat adapter; target epoch, peers, binary ranges and absent target remain
intact. No new dependency or feature is needed.

Go source: `step.go` lines 722–838 and `fastStepWaitDuration` lines 1175–1182,
frozen at `836fdc7cf3ef34f7eabc9d051418995bb97efabe`. Task 23 uses source-derived
assertions under the explicit user waiver of runtime oracle admission. No Go
execution or runtime differential is claimed. The adjacent `step_test.go` suite
was inspected and contains no MergeRegion/SplitRegion cases; the source itself
supplies these contracts. See [evidence](../../../docs/rust/schedule-execution/task-023.md)
and [fixture](../../../docs/rust/schedule-execution/fixtures/task_023.json).

An active merge always reports unfinished and emits a merge command. The passive
target emits no command and completes only when either boundary differs from its
original target range. Split uses the same boundary comparison against its stored
original range. Identity and epoch changes alone never complete either operation;
configuration version delta is always zero. Health/configuration do not reject
in-progress state; `Result<(), Infallible>` expresses this without invented errors.
An absent passive merge target is a programmer error and panics, matching Go's
direct pointer access. Active commands can preserve a nil target, and Display
uses zero IDs for absent metadata, matching protobuf getters.

Influence counts every metadata peer, including learners, witnesses and duplicate
stores. Split adds region counts and the matching peer-ID leader count; passive
merge subtracts them; active merge does nothing. Size, witness counts, step costs
and unrelated stores are unchanged. A missing leader has ID zero. Signed counts
wrap as in Go. Split reuses the verified fast-step timeout; merge multiplies it by
10 with signed wrapping (`timeout_nanos`). The unsigned `Duration` view preserves
the resulting two's-complement bits, consistent with previous joint steps.

The explicit test module `task_023_test.rs` checks frozen range/epoch cases,
complete command values for all policies and both V2 flags, binary/duplicate/empty
keys, target metadata, counts, nil and invalid inputs, overflow and timeouts.
`red.py` in the evidence directory reverses the split start-key comparison and
requires a behavior assertion failure, restoring the source in `finally`.

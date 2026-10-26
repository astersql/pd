# Leader configuration lifecycle

`BalanceLeaderScheduler::{update_leader_config,reload_leader_config,encode_leader_config}`
operate on the canonical scheduler's owned `BalanceLeaderConfig`. The host supplies the
canonical `StorageEndpoint` and `ClusterPaths`; the scheduler name selects its persistent
key. No HTTP types, private runtime, duplicate state cache, or global storage is introduced.

The domain replacement is already validated by `BalanceLeaderConfig::new`/`set_batch`:
1..=100, default 4. Callers clone snapshots to build changes; HTTP partial JSON decoding,
field recognition and status mapping remain adapter work. This API does not implement
the Go handler's partial-unmarshal side effects or accept arbitrary unvalidated disk state.
Reload uses the existing validated schema: storage/decode errors retain the active value.
The Go raw ReloadConfig does not validate corrupt batch values; accepting those values is
outside the typed domain contract inherited from task 070, and is not claimed as parity.

A changed valid replacement is published before persistence. Go's
`balanceLeaderSchedulerConfig.update` logs `save()` failure and still returns success;
it does **not** roll back a valid candidate. `LeaderConfigUpdate::Updated` exposes that
failure as `persistence_error` so hosts can report it. The active config still affects the
next scheduling round; reload can restore the older persisted config. An unchanged value
returns `Unchanged` without I/O. Exclusive `&mut self` prevents concurrent scheduling
or updates while these operations are borrowed. As with Go publication-before-save,
cancelling the save future can leave the active config ahead of persistence.

Encoding reuses the canonical key-range hexadecimal JSON representation and scheduler
encoding boundary. An encoding error occurs before publication. Persistence reads/writes
are scoped by cluster and scheduler name. This differs deliberately from the generic
`SchedulerConfig<T>` persistence-first update semantics; reusing it here would change Go's
observable behavior.

Evidence: [task-071](../../../docs/rust/schedule-execution/task-071.md).
Tests live in `task_071_test.rs` with explicit registration in `lib.rs`; six frozen cases
exercise actual leader commands after update and after reinstantiation/reload. I/O failure
uses `MemoryStore` behind a fault boundary. Expected values derive from read-only Go
source/tests, under the user's oracle waiver; no Go execution or runtime differential is
claimed.

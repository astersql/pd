# lifecycle.rs

`SyncLifecycle` owns one start/stop generation on a host Tokio `Handle` and supplied
`Downstreams`. Its private cancellation token is permanent: after cancel/stop,
new clients and downstreams return `Closed`; construct a new generation to restart.
The host registers `SyncMetrics` in its own `Registry` and passes it to the generation.
No private runtime, global subscriber, process handler, protobuf or tonic dependency.

`SyncAttempt` is the transport boundary for one follower connection/request/receive
session. `Complete` means EOF or leader loss; `Retry` or an I/O error waits for the
host-supplied retry duration. Each attempt and retry wait is cancellation-selectable,
with cancellation preferred before first poll. Dropping the attempt future must
cancel its I/O. Adapters own/reset `ClientApply` per stream and must not detach work.
Only one attempt runs per worker. Transient error text is transport-owned; adapters
choose Complete for terminal errors and Retry/Err for reconnectable errors.

Each downstream remains the existing exclusive-owner worker. `DeadlineSender`
bounds each Send; zero timeout uses Go's 10-second default. Timeout drops the I/O
future and terminates/removes the binding with a send error, without acknowledging
its frame or starting a second Send/close-message task. Transport implementations
must support cancellation by dropping their futures. Go's blocking Send tests
manually release their goroutines at the end; Rust deliberately closes on timeout
rather than leaving a transport Send running outside the joined worker.

`stop(&mut self)` cancels ALL workers before joining any, including both downstreams
and follower sessions. It returns every downstream/join failure after cleanup and
is idempotent. Awaiting handles by mutable reference retains unfinished workers if
the host drops a pending stop future; calling stop again completes cleanup. Drop
is cancellation-only fallback: hosts must await stop for proof of resource release.
Use one lifecycle as owner of its Downstreams; directly bound external tasks remain
their caller's responsibility. Finished handles remain owned until stop.

Metrics use the exact Go name/help/type and fixed `type` label values. Cached
handles avoid dynamic per-record label lookup. `sync_index` is the last assigned
history index, set on record (Go sets before increment); reset/reload leaves it
unchanged. `first_index` and `last_index` update on persist attempts, including a
failed save, not on each append or shutdown. To instrument `PersistentHistory`,
attach a clone of the same `SyncMetrics` to its `HistoryBuffer` before loading it.
No extra running/worker series or high-cardinality stream labels are introduced.

Independent tests in `lifecycle_test.rs` cover pending sends, deadlines, retry,
first-request cancellation, panic cleanup, real ClientApply/HistoryBuffer/Storage,
MemoryStore and EtcdMetaStore observation/persistence, and metric event semantics.

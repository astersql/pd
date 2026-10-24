# SyncRegions transport

`RegionSyncer` composes the canonical `BasicCluster`, `HistoryBuffer` and
`Downstreams`. The host supplies its Tokio `Handle` and shared cluster; accepted
region changes must use `publish` to serialize history append, cache publication,
and subscription capture under the cluster admission gate. This is an optional
adapter, not a replacement heartbeat admission algorithm or a running PD service.
It creates no private runtime, global metrics, MetaStore watch or backend.

`Downstreams::prepare_sync` reuses `plan_replay` and `FullSnapshot`. Zero, future
or unavailable history uses a full snapshot with positional batches of at most
1000 regions and a completion carrying the captured history boundary. An
already-current index gets an empty acknowledgement. Capture and binding happen
before another publish. Records after that boundary belong to the live drain;
completion describes the captured snapshot, not a later unrelated cache state.

The one-slot response channel bounds queued live frames. A readiness gate keeps
live sends behind initial replay; each downstream owns its independent index.
Successful channel acceptance is the local transport send boundary, not a peer
application acknowledgement. Network drop, channel closure and send timeout
terminate only the affected session. No I/O holds the history/cluster lock.

Each session has a tracked supervisor and one existing domain downstream worker.
Disconnect, same-name replacement, host cancellation and `stop` cancel and join
that worker. `stop` retains pending supervisor handles across cancellation of the
shutdown future; the host must await it before shutting down its runtime. Drop
requests cancellation as a fallback. A cancelled generation cannot be restarted;
leader generations use a new owner. Existing PD methods delegate through
`SyncGrpc` to the unchanged `grpc.rs`; the generated default SyncRegions method is
extended without editing the read-only reference.

`SyncFollower` implements the existing `SyncAttempt` boundary: connect, request at
caller-owned history index, reset stream readiness, then decode and apply each
response through canonical `ClientApply`, `Storage` and cluster state. The request
sender stays open through reception. Errors and rejected headers retry through `SyncLifecycle`; normal EOF ends the
generation, matching the Go client;
leader changes cancel and join the old lifecycle before starting the new endpoint.
All connect/receive/apply futures cancel on drop. Readiness describes the last
applied snapshot; the host lifecycle separately determines connection activity.

Tests use real tonic HTTP/2 loopback listeners and canonical follower containers,
MemoryStore and real EtcdMetaStore persistence, multiple readers, multi-MiB flow control, 1001-region
snapshots, publishes during scanning, reconnect and explicit shutdown. Only the
send-failure/deadline boundary uses a controlled real bounded channel. No Go or
protocol types enter the domain crate; no MetaStore persistence is added here.

`RegionSyncer::for_heartbeat_state` attaches directly to the canonical cluster
inside HeartbeatEffectsState via a private lock-scoped visitor. Its `downstreams()`
is passed to StatePipeline, whose effects publish once into that same history.
Full snapshot capture and binding use the state gate, also held by the sync effect
while appending. No copied BasicCluster or second history is introduced. In this
mode do not additionally call publish for pipeline reports; standalone new/publish
keeps its original contract. Queued effects must finish for a quiescent checkpoint;
subscription does not silently wait for unrelated runner queues.

The task52 test admits three real heartbeats, checks live wire-applied followers
and duplicate history suppression, then subscribes from zero and reads version3
from the same canonical root without an explicit publish/cache copy. It closes
streams, stops the RegionSyncer, joins the listener and stops all domain workers.

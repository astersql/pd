# downstream.rs

`Downstreams` owns the canonical `HistoryBuffer` and stream registry under one short
mutex. `append` records real immutable `Arc<RegionInfo>` values, observing the
slowest acknowledged index before each write so bounded history can grow. It
never acquires a sender lock or waits for network I/O. History capacity remains
bounded; an overwritten or future index fails explicitly as `HistoryGap`.

`bind` takes a host Tokio handle, a connection name, and its successfully sent
history index (including the result of full catch-up). A single worker owns each
`DownstreamSender`; its send future must cancel I/O on drop. `broadcast` coalesces
wakeups with `Notify` and a keepalive flag, storing no per-stream record queues.
Each worker snapshots at most 1000 pending records under the history lock, releases
the lock, sends, and only on success advances its own atomic index. It repeats
until current. An idle keepalive carries that stream's index; pending records
precede keepalive, and a send is followed by another history check even when no
new notification arrived. Complete region values retain their Arc identity.

Send/history errors remove only the failing binding. Registry cleanup uses Arc
identity so a replaced worker cannot remove its successor. Rebinding cancels the
old worker. The host cancels and joins every returned `DownstreamTask`; dropping
the handle also requests cancellation. Cancellation drops in-flight I/O, does not
acknowledge the incomplete frame, and releases the sender before join returns.
This module creates no private runtime and uses no protocol or MetaStore revision.
The host controls timeout policy in the sender adapter.

Tests in `downstream_test.rs` use real history/regions, controlling only the I/O
boundary with semaphores, send errors, and cancellation. They verify oracle event
sequences, independent progress, bounded batches, failed acknowledgements, history
gaps, keepalive races, replacement identity, and cancellation plus join.

The lifecycle owner can attach host-registry gauges to the canonical history and
await its task handles by mutable reference. This keeps pending joins owned if a
shutdown future is temporarily dropped. The lifecycle's DeadlineSender implements
per-Send deadlines without changing independent index/acknowledgement behavior.

`prepare_sync` captures an owned bootstrap plan from this same canonical history:
`plan_replay` for incremental requests, `FullSnapshot` plus completion for full
requests. `BootstrapFrame` separates positional batches from history frames. The
host must serialize cluster updates, preparation and the following `bind`; no I/O
runs in that interval. The transport can then send the captured values while live
records drain independently after its initial-send gate. `DownstreamTask::wait`
allows a tracked session supervisor to select worker completion against network
closure; after it completes the handle is dropped rather than joined twice.

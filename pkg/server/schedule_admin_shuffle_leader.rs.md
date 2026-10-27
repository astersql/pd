# Shuffle leader configuration transport

`serve(&ShuffleLeaderScheduler, Request<Body>) -> Response` is the canonical
scheduler-local boundary. It uses the existing domain scheduler; it adds no
storage, config mutation, runtime, feature, or alternate scheduler implementation.

The read-only Go `shuffle_leader.go` embeds `BaseScheduler` without overriding
`ServeHTTP`. `base_scheduler.go` writes `not implements`, implicitly returning
200 and `text/plain; charset=utf-8`. Every method and relative path receives the
same exact non-JSON body, including malformed or otherwise valid range updates.
No request stream is polled and no persistence is attempted; a storage failure
therefore does not turn this endpoint into a 500 or update its ranges.
The host owns route mounting and HEAD body suppression on the wire. Direct-handler
HEAD assertions retain the body, matching the scope of the Go method.

[Task 150 evidence](../../docs/rust/schedule-execution/task-150.md) explicitly
labels expectations as source-derived under user authorization, without claiming
Go runtime differential evidence. Independent tests check frozen HTTP cases,
unread external body faults, real MetaStore failure/recovery and preserved ranges,
and 24 source-derived scenarios before/after unsupported requests with both
heartbeat modes. Real cluster indexes, store filters, placement rules and
operator builders verify target/steps/kind/priority/influence/commands/completion.

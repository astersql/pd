# Witness leader configuration transport

`serve(&TransferWitnessLeaderScheduler, Request<Body>) -> Response` is the
scheduler-local transport boundary. It uses the canonical scheduler and adds no
configuration schema, storage dependency, runtime, feature, or router assembly.

Read-only Go `transfer_witness_leader.go` embeds `BaseScheduler` and does not
implement `ServeHTTP`, `EncodeConfig`, or `ReloadConfig`. The inherited handler
writes exactly `not implements`: status 200, `text/plain; charset=utf-8`, no JSON
or newline, independent of method, path, or malformed parameters. The inherited
configuration encoding is `null`; no HTTP update or persistence attempt exists.
Host routing and HEAD body suppression on the wire belong to the host. Tests
exercise the direct handler, including its retained HEAD body.

[Task 152 evidence](../../docs/rust/schedule-execution/task-152.md) labels the
frozen expectations as user-authorized source derivation, without claiming a Go
runtime differential. Tests cover methods/paths/invalid inputs, a stream that
panics if polled, real MetaStore I/O failure/recovery, and real queued scheduling
before/after unsupported updates. Healthy, pending/down, unavailable/budgeted,
learner/witness/demoting and missing-leader cases check errors, targets, kind,
Urgent priority, confver, influence, both heartbeat modes and completion. HTTP
requests preserve queued events and the three-event scheduling batch boundary.

# Diagnostic query boundary

`Manager::new` borrows the canonical `SchedulerController`, a current
`SchedulerConfigProvider`, and the host's actual recorder map. It neither duplicates
scheduler state nor creates recorders for unsupported schedulers. Hosts retain the
same `DiagnosticRecorder` objects used by scheduling and serialize mutation and
query access. Rust borrows keep one query's view stable; there is no second lookup
race after scheduler removal. Handler-only entries are not running schedulers.

`get_diagnostic_result(name, unix_seconds)` checks global enablement first, then
running scheduler existence and its **type** configuration, then recorder presence
and history. Missing/disabled schedulers return an empty Disabled result bearing
the requested name. Errors use the existing RFC catalog: diagnostic disabled,
scheduler undiagnosable, no diagnostic result. Successful history queries delegate
to the canonical recorder, preserving summary/status and query-time timestamp.
The host supplies Unix seconds, including boundary values, without private clocks,
Tokio runtimes or transport/protobuf coupling. Owned results cannot mutate history.

Source contract: `diagnostic_manager.go`, `diagnostic_recorder.go`, `errno.go`, and
`tests/server/api/diagnostic_test.go`, all strictly read-only. User authorization
allows source-derived expectations plus real Rust behavior/fault validation;
there is no Go execution oracle or claimed running differential. The 13 frozen
cases, provenance, real controller/MemoryStore tests, and fault recovery evidence
are in [task 111](../../../docs/rust/schedule-execution/task-111.md).

Host assembly of the recorder map into scheduler loops belongs to the subsequent
facade; this query crate supplies no automatic loop/HTTP wiring or persistence.

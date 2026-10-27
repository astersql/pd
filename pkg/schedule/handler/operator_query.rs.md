# Operator management queries

[operator_query.rs](operator_query.rs) provides `Handler<'a, C>` as a borrowed
view of an optional canonical `Coordinator<C>`. A missing coordinator maps to
`HandlerError::NotBootstrapped`; scheduler initialization is unrelated to cluster
bootstrap. Operator/status absence and empty record queries map to
`OperatorNotFound`. Both errors expose the Go RFC code and message. HTTP/JSON,
wall-clock conversion and response formatting belong to adapters.

`get_operator_controller`, `get_operator`, `get_operator_status`, `get_operators`,
`get_waiting_operators`, `get_admin_operators`, `get_leader_operators`,
`get_region_operators` reuse canonical admission/records. Kind filtering tests all
operator bits rather than primary scheduler kind. `get_operators_by_kinds`
concatenates groups without deduplication; an unknown kind reuses the previous
group, matching the Go switch without a default branch. Empty/unknown-only lists
succeed without requiring a coordinator. Within-group order is unspecified.

`remove_operator` first resolves the active identity, detaches with AdminStop,
acknowledges actual removed members against canonical store limiters, then buries
all returned members, including related merge operators. A racing replacement
cannot be removed by an older Arc. `remove_operators` uses the same acknowledgement
and burial composition for all active operations. Waiting operators survive.
No metadata writes, private runtimes or transport sends are introduced.
The controller's existing host serialization contract continues to apply.

`get_history` and `get_records` reuse ControllerRecords time/TTL behavior, including
inclusive finish-time filtering, direct status lookup at expiry, and strict record
enumeration at expiry. The caller supplies monotonic query time. Owned
`ArchivedOperator` values preserve Arc identity, frozen status and finish time;
`record()` exposes the existing OpRecord view without copying domain state.

Source-derived expectations come from read-only `handler.go`,
`operator_controller.go`, existing controller and API tests. User authorization
allows source-derived evidence plus real Rust behavior/fault verification. No Go
execution differential is claimed. See [task evidence](../../../docs/rust/schedule-execution/task-118.md)
and its fixed fixture for provenance and validation results.

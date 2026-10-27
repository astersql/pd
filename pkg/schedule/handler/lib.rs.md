# Schedule management crate

[lib.rs](lib.rs) is the canonical `astersql-pd-schedule-handler` entry point.
It exports the domain management Handler and HandlerError from
[operator_query.rs](operator_query.rs), forbids unsafe code, and explicitly
registers the separate task 118 behavior tests. Domain queries borrow existing
Coordinator state; the crate contains no server or wire adapter.
See [operator query contracts](operator_query.rs.md) and
[validation evidence](../../../docs/rust/schedule-execution/task-118.md).

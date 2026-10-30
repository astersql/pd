# Scheduler lifecycle HTTP

`http_scheduler.rs` maps the four classic `/pd/api/v1/schedulers` routes to
`PdServices::scheduler`. Creation uses Go's float64 object decoding; pause uses
signed int64 map decoding. Query status and timestamp presence follow the Go
handler, including an unknown status falling back to enabled schedulers.

The adapter validates transport input and preserves bootstrap/error precedence.
The host owns all policy instances, persistence and scheduling; no separate
scheduler registry or timer is retained by HTTP. Scheduler-local configuration
routes remain the next task's responsibility.

See [task 064 evidence](../../docs/rust/server-api/task-064.md) and the separate
`http_scheduler_test.rs` real TCP, heartbeat, restart and failure scenarios.

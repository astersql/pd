# Checker and scheduler diagnostic HTTP

`http_checker.rs` mounts GET/POST `/pd/api/v1/checker/{name}` and GET
`/pd/api/v1/schedulers/diagnostic/{name}`. Checker writes decode signed Go `int`
values and retain input error metadata, missing/negative delay errors, and the
pause/resume messages. GET returns the canonical pause snapshot.

`PdServices::checker` enters the existing term scheduling queue. The host borrows
the coordinator's checker controller and diagnostic manager, validates the leader
fence, and shares cancellation with the request. HTTP retains no checker state,
timer, scheduler or storage mirror. Disabled diagnostics and unsupported scheduler
errors remain observable. The scheduling worker records real gate decisions and
the first balance plan result per retry loop in the same coordinator history.

See [task 066 evidence](../../docs/rust/server-api/task-066.md) and the separately
registered `http_checker_test.rs` for real TCP/etcd, Go differential, cancellation,
follower forwarding, restart, and explicit pause-clock boundaries.

# Leader balance HTTP adapter

`serve` accepts an exclusively borrowed canonical `BalanceLeaderScheduler`, host-owned
`StorageEndpoint` and `ClusterPaths`, and an Axum HTTP request. It dispatches scheduler-local
`POST /config` and `GET /list`; route mounting in server/api remains the responsibility of
the host. It does not create a runtime, duplicate configuration cache or storage backend.

Updates overlay the current snapshot. Unknown fields are ignored alongside recognized
updates; a no-change request needs an exact recognized JSON tag. Decoding recognizes
case-insensitive tags, whereas no-change field recognition is case-sensitive, matching
Go's Unmarshal/FindSameFieldByJSON split. Batch is validated in 1..=100; out-of-range
updates return 400 and preserve the old config. Decode errors return 500 JSON strings.
Unchanged recognized updates return 200 without storage I/O. The body reader retains
bytes before an I/O failure, as Go's ignored ReadAll error does.

A valid changed snapshot goes through `update_leader_config`. Its publication-before-save
semantics mean persistence failure still returns 200 and keeps the new live configuration;
a later reload restores the older stored configuration. Storage and scheduler ownership,
including cancellation of the update future, retain the existing domain API's semantics.
JSON strings and objects retain their values; formatting whitespace and object-key order
are the only normalized response differences in the fixtures.

`BalanceLeaderConfig::set_ranges` preserves an explicit empty range list, distinct from
constructor defaults. Deserialization preserves it through reload. Scheduling retains the lower-level Go/Rust sampler's whole-keyspace behavior for
an empty list (with an empty registered key-range manager in this fixture). Nonempty
ranges continue to use canonical filter, placement, statistics and operator factory paths.
No new domain decision algorithm is introduced by this transport acceptance task.

Source contracts are `balance_leader.go` (handler, config update, clone and scheduling),
`base_scheduler.go`, `config.go`, `keyutil/keyrange.go` and `reflectutil/tag.go`, plus the
adjacent leader tests. The dedicated handler exists, so the BaseScheduler fallback
`not implements` is not the response of this scheduler. TestMain supplies leak/etcd
setup; ConfigClone tests ownership; BatchLimit tests the two HTTP batch boundaries;
the leader/range suites inform the actual scheduling scenarios. None is claimed as a
new Go execution. Evidence uses the user's authorized read-only Go source deduction
and Rust execution. See [task 137](../../docs/rust/schedule-execution/task-137.md).

Tests are explicitly registered in `lib.rs`, with four `task_137` functions in the
separate test file. They cover the frozen HTTP table, real scheduling and reload,
MemoryStore I/O failure and interrupted request bodies. This adapter retains the
validated domain boundary established in task 071: arbitrary Go partial-unmarshal
side effects that leave invalid domain state are not represented by this API.

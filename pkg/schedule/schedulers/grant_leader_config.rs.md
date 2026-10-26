# Grant leader configuration recovery

`grant_leader_config.rs` extends the canonical `GrantLeaderConfig` and
`GrantLeaderScheduler` from task 077. It uses `BasicCluster` OUT pause reference
counts, `StorageEndpoint` and its backend-neutral MetaStore contract; no transport,
new backend, runtime, feature or production dependency is introduced.

`from_args` follows `strconv.ParseUint(base 10)` and `getKeyRanges`: one store ID,
URL-decoded pairs, ignored unmatched trailing key, default full range. Persistence
uses the existing keyutil hexadecimal JSON range codec. Missing/null store maps
become an empty domain map; null range lists become empty vectors (same scheduling
meaning), serialized as `[]`. Numeric map order is deterministic; JSON object order
is not used as a semantic comparison.

`update_grant_config(id, ranges, ...)` takes typed domain input after transport
conversion. `None` reparses existing byte ranges through query-unescape, including repeated
`+`/`%` decoding; an empty stored list or `Some(&[])` selects full range. This
ports the handler's typed `[]string` path. Go's JSON decoder normally produces
`[]any`, which fails that handler's `[]string` assertion: ordinary HTTP ranges are
therefore ignored by Go. This API is not an HTTP JSON compatibility adapter; a
future adapter must preserve that conversion behavior explicitly.

A new store acquires one OUT pause. Parse failures resume it; for existing stores
Go also resumes OUT on range parse failure while leaving its map unchanged. Save
failure removes the target even if it existed before, resumes one OUT reference,
and leaves the old persistent snapshot intact. Rust holds a candidate map until
save completes; dropping a pending future releases newly acquired pauses. This
avoids exposing uncommitted memory through cancellation; it does not promise
rollback when an external backend commits and then loses its acknowledgement.

`remove_store`/`reset_store` implement membership/last-store reporting and best-effort
pause restoration. `delete_grant_store` saves first and publishes/resumes after
successful persistence (same final state as Go resume/save/reset). Non-final
deletes do not call the host removal callback. A final callback failure restores
memory and pauses, but leaves storage empty; NotFound retains removal. The callback
error remains primary even if restoration's pause fails. `reload_grant_config`
loads/decodes before changing pauses, reconciles membership only and publishes
missing-store entries while returning typed diagnostics. Retained IDs gain no
extra references. Task 077 `prepare`/`clean` consume the updated map unchanged.

Go sources are strictly read-only. Expectations come from `grant_leader.go`,
`utils.go`, `keyrange.go` and the adjacent scheduler test, under the user's
source-derived-oracle authorization. No Go runtime differential is claimed.
See [task evidence](../../../docs/rust/schedule-execution/task-078.md) and the
registered independent `task_078_test.rs` for failure detection, real MemoryStore
transactions, I/O failure injection, cancellation, callback and pause-state checks.

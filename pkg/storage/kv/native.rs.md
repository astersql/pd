# NativeMetaStore

The host provides an existing Tokio runtime, a peer network and a validated
OpenRaft configuration. A nonzero cluster and node identity owns the SQLite
directory exclusively until the Raft log and state machine release it. Use
`close` to cancel requests and join the lease and consensus tasks.

Linearizable reads use OpenRaft's applied read barrier. Serializable reads
inspect the locally applied state. A timed-out or cancelled submission can
still commit, so callers must resolve its outcome before retrying a mutation.
The application state and its backup/restore boundaries are described in
[native_state.rs.md](native_state.rs.md) and [native_backup.rs.md](native_backup.rs.md).

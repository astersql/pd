# Region heartbeat Go oracle probe

A Rust tonic client for the existing native Go pd-server, started by
`docs/rust/state/task48/oracle.py` with isolated ports/data. Reads cluster identity,
bootstraps store1/region1, sends accepted, duplicate, stale, invalid and nil-leader
reports, then reads GetRegionByID snapshots. Success silence is bounded by a 500ms
timeout and confirmed independently through the cached metadata. No Go source or
module is modified. Outputs and cleanup evidence are in task48/oracle.out.

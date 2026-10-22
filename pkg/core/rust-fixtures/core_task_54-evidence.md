# Task 54 verification

Batch 54 depends on batch 53, present as commit `447804069` with
`regions_stats.rs` and its registered tests. The Go region.go diff against
`0820f0b67b0e49b289b84fdf66b7b8fb178afe82` is empty.

## Behavior and evidence boundary

The JSON fixture records inputs, expected outputs and source derivations.
Read-only Go `TestGetRegionSizeByRange` was executed: 1 passed, exit 0.
Command: `go test -mod=readonly ./pkg/core -run '^TestGetRegionSizeByRange.*$' -count=1 -v`.
The selected test uses no failpoint injection; no make/failpoint transformation
was run. Go files and module configuration have no working-tree changes.
Other cases are source-derived Rust assertions, not executed Go/Rust differential
comparisons. The 1001-region Rust test covers behavior beyond Go's batch size 1000.

Public API additions on the existing RegionsInfo: get_region_size_by_range,
get_region_count_by_range, store_role_count_by_range, store_peer_count_by_range,
get_adjacent_regions, get_range_holes. Role counts delegate to existing rank
semantics (including reversed bounds and literal empty-end handling). Size sums
wrap like Go int64. Neighbor results filter holes and retain Arc identity. Hole
endpoints are uppercase hexadecimal; an empty registry returns no holes.
Rust holds a shared borrow for a complete scan rather than releasing the Go lock
between batches; this is a stable snapshot API, not concurrent batch interleaving.
No production fixture dependency, new crate, feature, or dependency was added.

## Commands

All Cargo commands used slot 1:
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`.
The slot was acquired by atomic mkdir and retained until all processes finished.

- Red: `cargo test --locked -p astersql-pd-core --lib core_task_54 -- --nocapture`:
  exit 101, E0599 missing public range APIs in registered tests (not an environment
  or manifest failure).
- `cargo fmt --all`: exit 0.
- Same focused test command: exit 0, 4 passed, 0 ignored.
- `cargo test --locked -p astersql-pd-core --lib`: exit 0, 142 passed, 0 ignored.
- `cargo metadata --format-version 1 --no-deps`: exit 0.
- `cargo fmt --all -- --check`: exit 0.
- `cargo check --workspace --locked`: exit 0.
- `cargo test --workspace --locked`: exit 0, 397 passed, 1 ignored across 74 test/doc-test result groups.
- `git diff --check`: exit 0.

Slot lock released after verification; compilation cache retained.

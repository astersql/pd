# SplitChecker

`SplitChecker::{default, get_type, check}` migrates the read-only decision in
`split_checker.go`. It reads the canonical `RegionInfo`, `SharedConfigProvider`,
`RuleManager` and persisted `RegionLabeler`; it does not own state replicas,
background work, storage writes or a runtime. Pause uses the existing
`PauseController`, with host-supplied Unix seconds.

The authoritative label index returns sorted, deduplicated keys strictly inside
the region. Nonempty label keys take priority and produce `labeler-split-region`.
Only when label keys are empty and placement is enabled does the checker query
placement and use `rule-split-region`. Keys from the two sources are deliberately
not combined: this follows Go Check and its TestSplit priority assertion.
No boundaries or an active pause produces `Ok(None)`.

The existing `create_split_region_operator` builds one `OpKind::SPLIT`, USEKEY
step retaining the region identity, epoch and range. Joint-state rejection
propagates `BuilderError::OriginJoint` without a partial operator. Go logs and
returns nil for this error; the Rust library retains the typed diagnostic for
its caller. No allocator or external I/O exists on this check path, so artificial
storage/allocator failures are outside this unit's scope.

Expectations in task_048.json are derived from read-only Go source and TestSplit,
not Go execution. The user's subsequent authorization permits that evidence;
there is no claim of Go/Rust runtime differential verification. See
[task evidence](../../../docs/rust/schedule-execution/task-048.md) for actual red,
green, mutation and workspace results. Tests live in `task_048_test.rs` and are
explicitly registered in lib.rs.

# progress crate

`astersql-pd-progress` is the canonical Rust crate beside `pkg/progress`.
It exports `Progress`, `ProgressIndicator`, `InvalidUpdateInterval` and `Manager`.
The indicator uses std only; manager consumes real core stores, host Prometheus
Registry and Tokio cancellation/timers. serde_json is a test dependency.

See [indicator](indicator.rs.md) and [manager](progress_manager.rs.md) for behavior
and boundaries. Tests are explicitly registered in `lib.rs` as separate
`task_130_test.rs`, `task_131_test.rs` and `task_132_test.rs` modules.

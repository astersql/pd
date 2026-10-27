# progress crate

`astersql-pd-progress` 是 `pkg/progress` 的 canonical Rust crate，公开
`Progress`、`ProgressIndicator`、`InvalidUpdateInterval`，实现说明见
[indicator.rs.md](indicator.rs.md)。仅有 std 生产依赖；serde_json 只用于测试读取
冻结场景。显式注册 `task_130_test.rs` 与 `task_131_test.rs`。manager、GC 和指标生命周期留给后续任务。

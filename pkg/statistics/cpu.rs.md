# CPU 负载归一化

对应只读 Go `cpu.go`，运行证据见[任务9](../../docs/rust/state/task-09.md)。
`store_read_cpu_usage(&[core::RecordPair]) -> f64` 只累加大小写敏感的
`unified-read` 前缀，包含完整前缀及任意后缀、重复记录；空集合为零。
先以 u64 回绕求和，再转 f64，保留 Go 溢出和舍入顺序，不转百分比。
`region_read_cpu_usage(Option<&core::PeerStat>) -> f64` 只读 unified_read，
peer 或 cpu_stats 缺失时为零，忽略 scheduler。不改变输入。

复用 canonical core 领域类型；无 protobuf、存储、运行时、全局状态或I/O。
Go nil RecordPair 对非空前缀无贡献，领域输入使用非可选记录，空或默认记录
保留这一零贡献行为。测试独立在 `cpu_test.rs`，由 `lib.rs` 显式登记。

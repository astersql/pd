# store_health 的覆盖补齐

store.go:308 WitnessScore 有符号加 delta 后转 f64；witness_score 使用 wrapping_add，保留零/负值与溢出。

只读 Go 源码为期望依据，没有新增 Go oracle 或运行跨语言差分。
独立相邻测试：`core_task_68_witness_score_delta_and_wrapping`；先缺 API 编译失败（101），再实现后通过。
组合写入另实际复现角色计数 0 != 1 的红灯。
命令、全量验证及编号责任见 [覆盖审计](../../docs/rust/core-coverage-audit.md)。

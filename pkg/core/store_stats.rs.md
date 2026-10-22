# Store 原始统计独立替换

`set_new_store_stats` 对应只读 `store_option.go:SetNewStoreStats`，
创建独立 `SharedStoreStats`，`average` 为 None；后续报告直接返回 raw available。
已有 `update_raw_stats` 仍更新共享容器，两个入口不能互换。

`core_task_68_new_store_stats_detaches_smoothing` 先因缺方法失败（101），实现后通过。
测试核验旧 clone 不受影响及连续更新不平滑；期望来自 Go 源码推导。
全部验证见 [覆盖审计](../../docs/rust/core-coverage-audit.md)。

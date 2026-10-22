# region_flow 的覆盖补齐

region.go:703/717 按 Store 投影大小及键数：非零 witness 返回零，其余返回整体统计。stats_mut 对应 region_option.go 的原始统计赋值，保留负值与来源。

只读 Go 源码为期望依据，没有新增 Go oracle 或运行跨语言差分。
独立相邻测试：`core_task_68_store_peer_statistics、core_task_68_raw_stat_options_preserve_signed_values_and_source`；先缺 API 编译失败（101），再实现后通过。
组合写入另实际复现角色计数 0 != 1 的红灯。
命令、全量验证及编号责任见 [覆盖审计](../../docs/rust/core-coverage-audit.md)。

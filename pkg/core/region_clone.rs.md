# region_clone 的覆盖补齐

region_option.go 的 ID、角色、witness/learner、down/pending、epoch 修改在 RegionOption 中按顺序执行；PeerIds 长度不符不修改，epoch setter 创建缺省另一字段为 1。

只读 Go 源码为期望依据，没有新增 Go oracle 或运行跨语言差分。
独立相邻测试：`core_task_68_clone_option_coverage`；先缺 API 编译失败（101），再实现后通过。
组合写入另实际复现角色计数 0 != 1 的红灯。
命令、全量验证及编号责任见 [覆盖审计](../../docs/rust/core-coverage-audit.md)。

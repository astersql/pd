# store_meta 的覆盖补齐

store_option.go 元数据修改通过 edit_meta 在浅 clone 后分离；set_store_meta 仅复制 Go 明确列出的字段，保留 ID、心跳和协议范围。

只读 Go 源码为期望依据，没有新增 Go oracle 或运行跨语言差分。
独立相邻测试：`core_task_68_store_metadata_options_detach_and_preserve_identity`；先缺 API 编译失败（101），再实现后通过。
组合写入另实际复现角色计数 0 != 1 的红灯。
命令、全量验证及编号责任见 [覆盖审计](../../docs/rust/core-coverage-audit.md)。

`set_network_slow_triggers` 对应 store_option.go 的同名 setter，保留 u64 值；
同一独立测试核验最大值及新旧 clone 隔离，先缺方法失败（101），再通过。

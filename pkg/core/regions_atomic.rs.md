# regions_atomic 的覆盖补齐

region.go:1128–1180 的组合写入维护 root 与角色子树；check_and_put_root_tree 仍只更新 root。stale 校验在写入前共用独占借用。

只读 Go 源码为期望依据，没有新增 Go oracle 或运行跨语言差分。
独立相邻测试：`core_task_68_combined_put_updates_roles_but_root_only_does_not`；先缺 API 编译失败（101），再实现后通过。
组合写入另实际复现角色计数 0 != 1 的红灯。
命令、全量验证及编号责任见 [覆盖审计](../../docs/rust/core-coverage-audit.md)。

# 最新区域统计收集桥接

`collect` 是 `pkg/cluster` canonical crate 的同步领域入口；宿主持有状态锁，
传入已有 BasicCluster、RegionStatistics、CheckerConfigProvider、RuleManager
及 Unix 秒，不创建 runtime、状态副本或 protobuf 依赖。

事件只提供 region ID；先从 root tree 重新查找当前 region，再检查取消，然后按
当前 region 的 peer store IDs 查询注册 store，调用真实 `observe_with_placement`。
缺失 BasicCluster 或 region 返回 `Ok(None)`，且不检查取消；取消返回类型化
`CollectCancelled` 并保持既有统计。未初始化 placement 时保留统计且返回 None，
沿用 canonical statistics 契约。没有存储写入，不引入人工持久化故障。

`server/cluster/heartbeat_effects.rs::submit_collect` 在已有 misc runner 中借用
同一份 host state 调用该入口。StatePipeline 的既有 heartbeat 提交路径继续使用
这条管线。安装 statistics 是宿主可选能力，None 时跳过；取消错误通过原有 receipt
机制报告，不回滚已接受的 root。没有新增第二条 heartbeat 路由。

Go 依据是 `pkg/cluster/cluster.go::Collect`、`pkg/core/basic_cluster.go` 和
`pkg/statistics/region_collection.go::Observe`；本任务使用用户明确授权的只读源码
推导 + Rust 实际行为/故障验证，不声明 Go 运行 oracle 或运行差分。
冻结来源哈希、输入/期望和实际日志见
[任务证据](../../docs/rust/schedule-execution/task-129.md)。

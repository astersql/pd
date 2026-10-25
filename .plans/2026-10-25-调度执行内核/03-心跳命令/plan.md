# 心跳命令与流计划

目标：迁移 `pkg/schedule/hbstream` 的来源行为并以运行差分、错误和故障证据验收。

范围：本目录编号任务定义的聚焦行为、局部注册与测试；总体范围及 Go 只读硬约束见[整体指南](../plan.md)。

范围外：其它包的业务实现、Go 改写、无验证的桩和新的配置/feature 扩展点。

假设：前置批次证据齐备；本指南只描述架构，进度只记录在编号任务文件。

## 设计决策

- canonical crate：`astersql-pd-schedule-hbstream`，按相邻目录组织，复用前序已验证接口。
- 采用行为级任务；不以整目录转换、编译成功或对象创建代替行为验收。

## 架构说明

- Operation 当前使用 pdpb/schedulingpb；Rust 领域命令与 wire 转换分开，hbstream 不反向依赖 operator。
- 共享入口串行修改，领域依赖不得反向到 server/runtime。

## 开发策略

- 先读[整体指南](../plan.md)及任务文件，再冻结 oracle、编写失败测试、最小实现、通过验证、重构。
- 所有任务默认 features；具体 Cargo 包、测试过滤与新增文件由各任务定义。
- 此 plan.md 及整体 plan.md 均只读；全局序号与批次在 prompt.md 中保持一致，每批只有一个任务。

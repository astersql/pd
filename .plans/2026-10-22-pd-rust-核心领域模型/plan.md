# PD Rust 核心领域模型计划

目标：把主题 2 分解为单一行为的小任务，逐步提供可被下游依赖的真实 Rust 核心模型与响应转换。

范围：`pkg/core/storelimit`、`pkg/core` 的 Store、Region、树索引、状态容器和 BasicCluster，以及 `pkg/response`；协议转换与阶段追踪放在现有 `pkg/server` 适配层。

范围外：完整调度、集群心跳服务接入、持久化、网络路由切换、全局 Runner 移植和 Go 工厂对象池机械翻译。

假设：规划基准为 `0820f0b67b0e49b289b84fdf66b7b8fb178afe82`。已有 workspace、叶子 crate 和 server/proto，尚无 core、storelimit、response crate；同名叶子能力仍须逐项核验。

## 设计决策

- 比较整包迁移、按文件迁移、按行为拆分；采用行为拆分，独立验证后提交，避免大文件变成大任务。
- 使用 canonical crate；新增模块留在 Go 包旁，协议及指标宿主装配留在 server。

## 架构说明

- `server -> response/core -> storelimit/叶子`；core 禁止 proto、transport、storage 依赖。
- 显式时钟、取消和随机源；不创建私有运行时或全局注册器。

## 开发策略

- 对会改变行为的代码，使用失败验证-通过验证-重构。
- 在实施步骤前规划失败验证命令和预期失败。
- 将实施步骤限制在失败测试所证明的行为范围内。
- 测试真实行为，不验证模拟对象或实现细节。
- 依赖、线性顺序与批次均以编号任务和 prompt.md 为准。所有批次串行，避免共享 lib.rs、Cargo.lock、格式化及 Go failpoint 改写争用；Cargo 槽位仍与其他计划共用。
- Go 与 Rust 读取同一小型 JSON 场景 fixture，分别计算并比对期望；禁止手抄 Rust 结果冒充 Go oracle。浮点仅在明确精度后比较，时间与随机只归一化非业务差异。
- 文档和最终审计不代替行为迁移；未满足依赖必须保留阻塞证据。来源史诗与上层 plan.md 保持原样，只有最终回归任务可按其完成条件关闭来源史诗。

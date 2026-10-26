# 范围散射调度领域

`ScatterRangeScheduler` 复用 `RangeCluster`、`BalanceLeaderScheduler` 和
`BalanceRegionScheduler`，不复制均衡算法。`schedule` 借用宿主真实规则、统计、
亲和性、keyrange、operator influence 和分配器。随机输入、时钟与 I/O 由边界提供；
不安装 runtime 或全局状态。HTTP/metrics/registry 和 operator 提交留给适配层。

每轮以当前配置的原始 UTF-8 字符串键建立范围快照，只把配置的局部副本的
`tolerant_size_ratio` 改为 2，不修改宿主配置。leader 子调度器先运行；仅在无操作时
再运行 region 子调度器。两者各自检查同一个 OpRange 数量与各自严格小于的限额。
子调度器名称为 scatter-range-leader/region，保留真实 retry quota，跳过普通
balance 调度器的 keyrange exclusion。region 允许小区域。Go rangeCluster 仅覆盖
leader/follower 抽样，pending/learner 仍继承父集群；本实现沿用已有范围视图契约。

仅返回批次中的第一个 operator 加 RANGE kind 并改 description，其余 leader 批次项
不变。`Operator::decorate` 是操作器发布前的独占修改入口，保留步骤、优先级、
影响量、健康标记和 additional_info。子引擎新建的 Arc 在返回时独占；不修改已提交操作器。
领域结果保留类型化 BuilderError 便于宿主诊断，plans 与 Go 一样丢弃。

配置 builder 参数顺序 name/start/end，要求恰好 3 项；不增加 Go 不存在的非空、
键顺序、URL 解码校验。初始实例名为 scatter-range-scheduler-{name}，reload 可更换
配置中的 range-name，但不改实例名或持久化路径。更新拒绝改名，先改本地键再存储；
I/O 错误后本地新键保留，持久化旧值不变。reload 仅在成功读取/解析后整体发布。
StorageEndpoint 使用宿主 MetaStore，本任务测试使用真实 MemoryStore，仅在 read/
transact I/O 边界注入 Unavailable。

预期依据为 Go scatter_range.go、range_cluster.go、balance_region.go 和已有
TestScatterRangeBalance 的只读推导。用户授权以源码推导加 Rust 行为/故障测试验收；
没有 Go 运行 oracle 或运行差分。源码哈希、48 条行为案例、红灯和全量验证记录见
[任务证据](../../../docs/rust/schedule-execution/task-099.md)。

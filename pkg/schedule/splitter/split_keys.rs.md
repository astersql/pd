# split_keys.rs

`group_keys_by_region` 读取 canonical `RegionsInfo`，复用
`RegionReplicationContext` 与 `is_region_replicated`，返回按 ID 排序的
`RegionGroupKeys`。组中保留原始 `Arc<RegionInfo>`，keys 为独立字节副本。
宿主保证区域、配置、stores、RuleManager 属于同一代输入，并消费
`add_suspect(false, id)`；复制不足时按每个 key 通知，包括重复和边界 key。

来源为同目录 `region_splitter.go` 的 `groupKeysByRegion/checkRegionValid`。
Go 实际保留重复 key 和输入顺序；本任务名称中的“去重”不解释为改变这个契约。
仅跳过已为区域起点的 key，区域按 ID 唯一归组。区域 end 为排他边界，空 end
表示无限，空洞不能匹配下一 region。使用 canonical scan 后再检查 contains，
不复制区域索引。每次调用都重新查询，旧 Arc 可保留但不会成为下次 lookup 缓存。

判定顺序：查询 → 复制检查（失败通知 suspect）→ leader → start-key 排除。
Go 以跳过表达缺失区域、复制不足、无 leader 与已拆边界，并不返回 error；
本 API 同样不将这些过滤结果变成请求失败。没有新增 I/O、运行时或持久化行为。
复制检查不是通用健康检查：legacy 模式要求精确副本数且无 learner；rules 模式
使用真实 RuleManager fitting。pending/down peer 本身不否定满足规则的 region。
Go map 迭代未规定顺序，本 API 明确升序 region ID，组内顺序不归一化。

实现范围止于分组和有效性检查。operator 派发、扫描新 region、完成率和重试由
后续任务负责；返回组不表示 split 已成功。

[任务证据](../../../docs/rust/schedule-execution/task-062.md) 包含源码哈希、fixture、
失败与通过记录。遵循用户授权，期望源于只读 Go 源码与测试契约，未运行 Go，
没有声称运行差分。测试使用真实 core 索引、replication filter、RuleManager 和
MemoryStore 初始化，无模拟分组或注入生产桩。

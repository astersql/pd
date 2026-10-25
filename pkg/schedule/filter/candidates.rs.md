# StoreCandidates 与比较器

对应只读 Go `candidates.go` 和 `comparer.go`。生产实现位于同目录
[candidates.rs](candidates.rs)，从 canonical filter crate 导出，不新增 crate 或依赖。

`StoreCandidates` 接收 `Vec<Arc<StoreInfo>>`，拥有候选次序而不复制 store 状态；
`pick_all` 返回借用切片，所有 pick 返回原 Arc 的借用。空集合返回 None，完整
u64 ID 和重复项保留。source/target 链式方法调用批次 5 的真实 selector，继续使用
canonical 配置和 Collector 的首拒绝、资源步骤语义。

`sort` 升序且不承诺等键稳定性，与 Go sort.Slice 的契约相同；它要求调用者提供
一致的比较关系。`keep_the_top_stores` 逐步交换并截断，保留 Go 的顶部并列项次序；
`pick_the_top_store` 不重排，遇到并列保留最先出现者。比较器返回 Ordering。
RegionScore 比较器借用同代配置，传递 version/high/low 与 delta=0 到真实 core；
Isolation 比较器调用真实 distinct_score，保留 replica 的重复项和 ID 排除规则。
浮点评分按 Go 的 >、<、否则相等处理，不使用改变 NaN 比较含义的 total_cmp。

随机来源由宿主传入 `FnMut(usize) -> usize`，参数为 exclusive upper bound；宿主
负责提供均匀样本，不安装全局 RNG 或运行时。shuffle 使用下降 Fisher–Yates，空和
单项集合不取样；random_pick 在单项集合仍请求 bound=1，空集合不取样。
Go 的内置 RNG 不会产生越界索引，Rust 的外部随机源可能违反契约，因此在该输入
边界返回类型化 `RandomIndexError`；不会越界访问或隐藏错误。shuffle 若后续取样
失败，保留此前有效交换；调用者可以检查集合并重试。这里没有持久化或外部 I/O。

[任务 006 证据](../../../docs/rust/schedule-execution/task-006.md)记录真实 Go
测试/返回分支、23 个比较场景、一次同对象并列返回、候选检查点和随机取样 tape。
随机差分重放实际 Go 样本，不声称不同 PRNG 的同 seed 序列一致。Rust 独立边界
测试验证空/单项、完整 ID、并列、不重排、Arc 身份、筛选链及越界随机源的恢复状态。

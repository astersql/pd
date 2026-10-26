# split_results.rs

`split_region_by_keys` 用 canonical `create_split_region_operator` 构建
`region-splitter` / `OpKind::SPLIT` / `SplitPolicy::UseKey`，保留 epoch、原范围与
二进制 keys，交给真实 `ControllerAdmission`。`SplitExecutionError` 分别保留
构建与准入错误。返回 `AdmissionOutcome` 由宿主现有 dispatch 消费；该 API 不创建
第二个 controller、状态索引、runtime、后台任务或 wire adapter。

`SplitAttempt::submit` 消费批次 62 的 `group_keys_by_region` 结果，只保留成功
准入的组。返回 `SplitSubmissions` 包含宿主待派发 outcomes 和逐 region 类型化错误。
宿主每个 watch tick 以同一 canonical 状态面的最新快照调用 `scan`，不能把提交或
派发成功计为拆分完成。重试、100ms watch cadence、1min timeout 与取消属于任务64；
当前组件只实现单次提交与可重复扫描，不内置时钟或阻塞等待。

扫描使用原始 region `[start,end)` 范围、无限 limit，匹配当前 region start 与
原始 split keys。不额外要求 epoch 增长、ID 不同、leader 存在或复制健康；Go
`ScanRegionsByKeyRange` 没有这些检查，原 ID 可以作为右子 region 被观察到。
只有 epoch 改变而未出现匹配 start 时，不产生结果。空 end 沿用 canonical 无界扫描。
`SplitKeyResults` 跨 tick 累积 ID→start，后次同 ID 更新会覆盖旧 start。

`SplitGroupProgress::finished` 比较**本次扫描**的唯一 ID 数与原始 key 数，随后锁存。
因此重复 key 可以都从 unprocessed 中消失，却仍不能 finished；分次累积出所有
key 也不必然 finished。这是来源的两个不同判据，不能合并。`unprocessed_keys`
按输入顺序保留未观察 keys 的重复项。已 finished 的组不再扫描；拒绝的组也不扫描。
空 accepted groups 可 finished，但不表示所有输入 keys 成功，必须读取 results。
分组复制不足的 suspect 回调仍由批次62执行，扫描阶段不另行标记 suspect。

结果与组使用 BTreeMap 固定 Go 未指定的 map 迭代顺序，其余字段不归一化。原始
`RegionGroupKeys` 必须来自同一代配置、规则及 cluster 状态，宿主串行提交/观测；
不要在扫描间修改进度对象持有的原始 group。无持久化或外部 I/O，未新增存储故障点。

[任务证据](../../../docs/rust/schedule-execution/task-063.md) 与
[固定场景](../../../docs/rust/schedule-execution/fixtures/task_063.json) 明确标注
期望来自只读 Go 源码与测试契约，未运行 Go，未声称 Go/Rust 运行差分。

# 操作终态记录与成功通知

`ControllerRecords` 是 canonical operator 的领域记录层；持有 `Arc<Operator>`，
不复制 admission 状态、限流器、cluster 或 heartbeat transport。宿主把
`AdmissionOutcome.removed`、移除 API 返回值及拒绝批次交给 `bury_removed`，
把 `DispatchOutcome` 交给 `record_dispatch`。后者只消费实际 `removed` 成员，
不会因为竞争中落败的 `completed` 单独通知。merge partner 按自己的终态记录。
成功后的 nonwitness TTL、fast influence 和等待队列晋升继续由派发层和宿主处理。

`bury_operator` 按操作对象身份去重，同 region 的新操作可以覆盖旧记录。
弱引用 tombstone 不延长操作生命；下次归档清除已释放对象的 guard。
TTL 到期或 `clean_all_op_records` 仅清理可查询记录，不能令仍被重试的操作
再次通知。记录发布和去重在同一互斥区内；成功回调按配置顺序在解锁后执行，
允许重入查询/重复归档。`set_success_callbacks` 在共享 owner 前替换列表，跳过 None。
宿主回调 panic 向调用者传播，已发布状态不回滚、不污染记录锁，也不会自动重试
回调；本层不承诺 panic 后继续执行余下回调或跨进程 exactly-once。

`ArchivedOperator` 固定状态、完成时间，`record()` 复用既有 `OpRecord` 的持续时间
规则。`get_status` 优先查询真实 admission，然后查询每 region 的最近归档。
`get_records` 的 from 条件包含等号；TTL 十分钟，direct lookup 在精确到期时
仍有效，但 key enumeration 在该时刻排除，保留 Go TTL Get/GetAllID 的细微区别。
`get_history` 包括失败操作，历史时间为查询时间而非归档时间。返回集合无排序承诺。
时间参数由宿主单调时钟提供，无私有 runtime、GC 线程或持久化后端。

`TerminalObservation` 为宿主提供每次归档唯一的指标事件：finish/replace/expire/
timeout/cancel，非终态为 unexpected 并尝试 Unknown 取消。仅成功事件提供 running_time，
用于 Go `operators_count{type,event}` 和 `finish_operators_duration_seconds{type}`
对应的宿主 exporter。操作 finished counters 可注册为成功 hook；本层不安装全局
Prometheus registry，也不增加新指标名或新 label。复用既有类型，无新增依赖。

来源为只读 `operator_controller.go` 的 SetSuccessCallbacks、buryOperator、
GetOperatorStatus、GetRecords、GetHistory、CleanAllOpRecords 和 records，及 metrics.go、
cache/ttl.go。源码推导证据与测试见 [task-037.md](../../../docs/rust/schedule-execution/task-037.md)。
用户已解除 Go 运行 oracle 准入限制；本任务没有 Go 运行差分声明。

# 桶报告接受与热点接线

`handle_bucket_heartbeat` 对应只读 Go `RaftCluster.processRegionBuckets` 与
`HandleRegionBuckets`，调用方持有 `BasicCluster` 独占锁直至状态提交和worker提交结束。
只接受领域 `BucketReport`；不依赖 protobuf、传输、调度决策或MetaStore。

- 缺失region返回可比较的 `BucketRegionNotFound(id)`，显示 `region N not found`。
- 注册的leader store处于Removed时返回 `Ok(false)`，不更新报告、不提交热点任务。
  缺失leader或缺失store按Go语义继续接受。
- 使用真实core `compare_and_set_report_buckets`；更高版本替换独立report，低/同版本
  返回成功并保留原对象。heartbeat bucket metadata继续优先显示。
- 新report通过 `RegionsInfo::put_region` 同时更新ID/root/role索引；所有range、epoch、
  peer与统计字段保持当前快照，已持有的旧Arc仍为旧快照。低/同版本不替换索引Arc。
- `retry_bucket_update` 最多执行三次CAS，任意成功立即停止；全失败不发布状态。
  当前core独占借用使生产CAS无法被中途打断，正常一次成功。独立策略测试以真实
  `RegionInfo::update_buckets` 和过期Arc身份产生失败，不伪造core版本结果。
- 返回true表示Go的成功接受语义，包含未替换的低/同版本；这类报告依然提交完整flows
  与毫秒period给真实 `BucketWorkerClient`，不把“版本没替换”混同Removed忽略。
- worker可选，由宿主决定是否本地处理热点。队列full/closed时尽力提交后仍返回接受成功，
  与Go `CheckAsync`一致；不存在元数据回滚，也不把queue admission当作持久化保证。

测试在 `bucket_heartbeat_test.rs` 显式注册，覆盖missing/Removed不入队、版本与实际worker
负载、metadata优先和分裂继承、跨线程两种提交顺序、真实core指针CAS三次边界、关闭worker
后保留core更新、缺失leader/store允许。worker由宿主Tokio Handle启动，测试显式shutdown
取消并join，重复shutdown安全；生产函数不新增后台任务。

Go运行oracle、红灯、回归及只读证据见 `docs/rust/state/task-45.md`。

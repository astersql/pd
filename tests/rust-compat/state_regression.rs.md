# 状态面联合差分验收

`state_regression.rs` 只在 compat crate 的 cfg(test) 中装配公开 canonical 组件。
Host 持有 BasicCluster、HeartbeatEffectsState、三个 ConcurrentRunner、HotStat、
StorageEndpoint 和唯一 Downstreams；默认 feature，无 operator/checker/scheduler 实现。
bootstrap store 的 metadata/rolling entry 与真实 Go 服务启动状态一致。

读取真实 Go 服务采集的 task-53.json：保留 protobuf 输入 bytes，逐步重放 region、
store 与 bucket 事件，等待 effect receipts 和 HotStat FIFO 屏障后比较整个观测快照。
快照包含完整 HTTP RegionInfo/热点/bucket 返回值、原始 StoreStats/StoreMeta、KV
RegionMeta、全量同步帧、持续增量和断连追赶帧；未知或额外字段不会被丢弃。

SyncRegions 仅对区域集合排序，连带 stats/leaders/buckets 保持对应关系；所有帧
index、完整 region/bucket protobuf 字段快照（prost 规范化编码）、raw counters、周期均保留。JSON 的整数与整数浮点
词法归一到同一数值，2^53 外整数不经 f64 改写，不设数值容差。热点数组仅按 region
ID 排序。StoreMeta.LastHeartbeat 与 read peer 更新时间使用明确的宿主时钟原点；
write peer 时间来自实际 RegionInfo.report_interval，移除区域的零时间保持原样。

Go stale 错误原始文本留在 fixture；领域比较使用稳定 error type 加 incoming/current
完整 metadata。current 来自同一 checkpoint 的实际 Go SyncRegions 帧，不从 Rust
结果生成期望，也不把 Rust Debug 文本与 gogo protobuf 文本当作相同的格式承诺。

回放结束先 cancel/join 下游，再 stop 三个 runners 和 HotStat.shutdown；关闭后
snapshot 必须报错。故障测试另使用真实 MemoryStore/EtcdMetaStore，覆盖 revision
恢复、compaction 对账、持久索引重载、真实 Etcd stop/start、阻塞发送取消/join。
共用 harness 的唯一 etcd fixture 模块，避免多个静态计数器生成相同 WAL 目录。

验收器曾用 epoch.version=999 的错误期望副本产生精确路径红灯，最终测试断言该差异
能够被检测；生产实现未为红灯被破坏。必要扩展修复了 source bucket 报告的 raw
stats/period 丢失，Go follower 仍按 SetBuckets 只消费 metadata。
证据与来源覆盖表见 [任务53](../../docs/rust/state/task-53.md)。

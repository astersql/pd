# Region 来源与 bucket 报告快照

`RegionInfo` 通过 `RegionSource` 区分 storage/sync/heartbeat；继承仅修正未初始化
size/keys，并在宿主开启 bucket 功能时继承独立 report。heartbeat BucketMeta 优先。

`RegionBuckets` 保留 region_id、version、ordered keys，以及可空的
`RegionBucketStats` 六组原始累计计数和 `period_in_ms`。这些是 owned 值，不依赖
protobuf、统计 crate 或 I/O；每秒负载仍由统计 worker 唯一计算。
metadata getter 构造的有效 buckets 的 stats=None/period=0；report getter/cloning
保留原始细节。版本/CAS 规则不变，克隆不共享可变计数数组。

任务53的真实 Go 服务输出证明独立 report 的 full-sync 帧携带 stats/period；
Go follower SetBuckets 仅消费 BucketMeta。不能因此在 source 转发时丢弃报告字段。
差分与故障证据见 [任务53](../../docs/rust/state/task-53.md)。

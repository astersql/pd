# 全量同步追赶

`FullSnapshot::catch_up` 消费已发送的快照并持有其 retain，直到完成或失败。
在宿主 append 共用的 `Mutex<HistoryBuffer>` 下调用 `retained_records_from`，固定
完整半开区间 `[sync_start_index, next)`，释放锁后分帧发送真实 `Arc<RegionInfo>`。
每帧最多 1000 条。非空快照追赶使用 history index；空快照追赶使用从0开始的位置，
completion 独立携带真实 history next。两者通过 `CatchUpFrame` 变体区分。

仅成功发送所有追赶帧和 completion 后返回 live drain 起点。发送中追加的历史
不改变 completion 上界，后续使用已有 `plan_replay` 发送这些记录。窗口已被覆盖
返回 `FullCatchUpError::ResyncRequired(HistoryGap)`，不发送任何追赶或完成帧；
宿主应映射为 ResourceExhausted，并由必传 disconnect 回调移除并关闭当前流。
任意发送错误也调用 disconnect 一次，停止后续发送，不返回成功 acknowledgement。

宿主持有流发送锁，防止追赶与下游发送交错；发送回调负责检查 open/cancel 与限速。
这里不创建后台任务，不安装运行时，不访问MetaStore或protobuf，也不管理注册表。
回调 panic 的宿主清理由宿主 RAII 负责；快照 retain 在 unwinding 时自动释放。

[行为证据](../../docs/rust/state/task-35.md)。

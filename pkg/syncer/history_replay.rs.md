# 增量历史重放

`HistoryBuffer::plan_replay` 将请求起点、绑定时捕获的结束索引和非零批次大小
转换成 `HistoryReplay`。复用现有历史区间和窗口策略，不把MetaStore revision当作序号。

- start=0 或 start>end 返回 `FullSync`，携带捕获的end供后续全量/追赶接线。
- 非零start<end先观察所需窗口；窗口增长不能复原已丢失记录。
- start=end返回一个空确认帧，即使已捕获end之后又有新记录，也不提前发送。
- 仅在完整保有[start,end)时产生增量帧，否则返回 `FullSync`，不发送部分历史。
- 每帧至多 min(调用者批次大小,1000)条；start为第一条历史序号，end为排他末尾。

`SyncFrame` 持有完整不可变 `Arc<RegionInfo>`，保持字段与记录顺序；快照在覆盖、重置后仍有效。
协议的meta/stat/leader/buckets转换由server适配层完成。本模块无protobuf、网络、后台任务和存储I/O。
不推进下游游标；宿主只能在成功发送后确认帧end，失败/取消时保留最后成功确认索引。
同crate的下游drain在核对完整历史区间后通过内部 `from_records` 复用帧类型。
`SyncFrame::keepalive` 只接受宿主已确认的发送索引，不能将history.next当成未发送记录的确认。

真实领域行为与Go运行基准见[task-33](../../docs/rust/state/task-33.md)。
全量发送、追赶、发送超时及关闭流由后续任务接线；本任务的FullSync只是明确领域决策。

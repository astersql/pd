# 全量快照边界

对应只读 `server.go` 的 `bindStreamForSync` 与 `syncFullRegionsLocked` 扫描部分。
`FullSnapshot::bind` 在宿主 append/bind 锁内取得独占 HistoryBuffer 借用：调用方须将
整个 append 批次放入同一锁。捕获 nextIndex 后立即 retain，再从真实 BasicCluster
取得 Arc 快照。绑定不会重新分配历史序号，不保存锁或借用以阻塞发送期间的 append。

`SnapshotOffset(usize)` 与 `HistoryIndex(u64)` 是不同类型；全量帧只公开前者，
不能调用增量 SyncFrame 的 end_index 将扫描位置冒充下游确认。批次大小非零，最多1000，
无序 regions 保留全部领域字段；发送期间 cluster 改变不会修改已捕获内容。
空快照无扫描帧，历史边界仍保留，后续追赶/完成必须使用该边界而非region数量。

`send` 的宿主回调负责开放检查、限速和取消，任何错误立即停止后续帧，返回原始错误并
释放 retain；panic 也由 RAII 释放。成功返回 FullSnapshot 所有权，必须保留至追赶完成
后丢弃。retain 有历史容量上界，宿主仍需检查 retained_records_from 的 HistoryGap；
不可将保留句柄当作无限历史保证。没有后台任务、存储副作用、私有runtime或协议依赖。

当前 API 的 bind 与扫描连在一个同步调用中，覆盖append边界；尚不承担流注册、
增量回放、追赶完成帧或协议状态映射。这些后续接线必须继续保留捕获的历史边界。

行为证据见[任务34](../../docs/rust/state/task-34.md)及独立 full_snapshot_test.rs。

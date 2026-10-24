# 客户端全量就绪状态

`ClientReady` 为 `ClientApply` 内部唯一的 running/full_syncing 状态持有者。
来源为 `client.go` 的 `handleRegionSyncResponse`，运行 oracle 和逐帧状态见
[任务38](../../docs/rust/state/task-38.md)。不暴露 protobuf、传输、运行时或存储后端。

`classify` 只读分类，`complete` 在区域应用成功后提交状态，避免把本地 history 错误
当作帧成功。首次未 running 的 start=0 非空帧进入 full sync；第一帧允许重置历史索引，
其后非空帧携带位置，不提交这些位置为历史索引，也不记录全量区域到历史。
空帧提交最终历史索引并结束 full sync，包括旧 leader 的 keepalive。
空快照产生的第一批追赶位置从0开始，同样等待空完成帧；没有追赶记录时空帧立即就绪。
已 running 的 start=0 数据帧仍是普通增量索引重置。

错误响应只清 running，保留 full_syncing 和历史；宿主重连时调用
`ClientApply::reset_stream` 清两个标志，保留调用方持有的缓存及历史。
任务39负责启动/取消/重试等宿主生命周期；本模块没有后台任务或 wall-clock 等待。

独立 `client_ready_test.rs` 显式登记，通过 `ClientApply` 验证真实区域索引、HistoryBuffer、
MemoryStore 和 EtcdMetaStore，1001区域场景复用 FullSnapshot/catch_up 产生真实领域帧。

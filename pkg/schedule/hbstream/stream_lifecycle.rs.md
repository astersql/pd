# 心跳流保活与停止

`HeartbeatStreams::start` 接收宿主 Tokio `Handle`、真实 `StreamRouter`、cluster ID
和宿主注册的 `Arc<HeartbeatMetrics>`。只有一个串行消费者，不创建私有 runtime，
不派生独立发送任务；消息和绑定队列延续任务13的1024/1槽。

一分钟后开始保活，使用 Tokio interval 的 Skip 策略跳过积压周期。
`HeartbeatMessage::KeepAlive { cluster_id }` 明确表示仅 header 的响应，避免用零值
peer 冒充不存在的 target。协议分支和 wire 转换属于适配层；本层不依赖 protobuf。
`MessageStream` 提供可背压的领域消息 channel 传输。原命令 channel 保持兼容；
其默认保活返回 `KeepAliveUnsupported` 类型错误，完整适配器须实现 `keep_alive`。

取消先唤醒等待命令/绑定队列的生产者并保留失败命令。消费者每次分派前检查取消，
不排空积压命令。`stop` 和 `join` 可重复调用；丢弃 pending join 不丢失 JoinHandle。
`join` 等待正在执行的 send，包括保活发送，随后释放路由器、队列和 ticker。
这与 Go `Close` 的 WaitGroup 一致：传输边界必须使 I/O 最终返回，stop 本身
不强制中止已开始的外部发送。Drop 请求取消；宿主关闭屏障使用 stop 后 await join。

保活检查真实 StoresInfo；缺失/Removed store 删除绑定，发送失败删除绑定且不重试。
指标保持 `pd_hbstream_region_message`、Go help 和 address/store/type/status labels，
push 使用 ok/err/skip，keepalive 使用 ok/err。缓存 counter handles，删除/reset
与缓存失效共用锁；不使用全局 registry。重复注册返回 prometheus 类型错误供宿主处理。
store 移除由宿主调用 `delete_store_metrics`，reset 使用 `reset_metrics`。
发送后重新读取 store，避免阻塞期间 bury 后重建指标；沿用 Go 的顺序语义，
不额外承诺 store lifecycle 与 metrics 的跨对象事务原子性。

Go 来源是相邻 `heartbeat_streams.go` 和 `metric.go`，只读运行旧二进制。
[任务证据](../../../docs/rust/schedule-execution/task-014.md) 说明五组 Go 运行观察、
饱和关闭、错误变异和宿主共享运行时验证。测试独立注册在 `task_014_test.rs`。

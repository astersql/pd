# 心跳流路由

`StreamRouter::new(Arc<StoresInfo>)` 返回可克隆 `RoutingHandle` 和宿主驱动的单消费者。
领域命令复用 `HeartbeatCommand`；没有 wire/operator/server/runtime 反向依赖、私有
运行时、后台任务或存储副本。`HeartbeatStream::send` 是唯一外部传输边界，默认实现
支持真实 Tokio `mpsc::Sender<HeartbeatCommand>`，错误保留在 `RouteEvent::SendFailed`。

`bind` 排队更新，容量1；成功入队不代表已经生效。`step` 应用更新时替换同store的
旧流。命令队列容量1024，FIFO；两个队列之间选择不保证先后，与 Go select 一致。
调用者需要等待绑定生效时，可驱动 `step` 观察 `Bound`。`send` 等待容量，`try_send`
区分Full/Closed且保留原命令；关闭consumer后发送返回错误。两个入口都关闭且队列
耗尽时 `step` 返回None。

路由从共享真实store registry读取当前快照。缺失或Removed时删除绑定并丢弃命令；
未绑定时跳过；成功发送保留绑定，失败删除且不重试。发送等待期间不会应用重绑。
已开始的发送可能跨越store移除；与 Go 一样，成功发送不撤回，下一命令再次检查。
取消一个正在传输中的 `step` 会放弃该次失败清理，因此宿主需等待发送完成；保活、
取消/join和指标由任务14接续，不在此处创建第二个runner或新的feature。

[任务13证据](../../../docs/rust/schedule-execution/task-013.md) 记录实际 Go 输出、
二进制/源码/fixture哈希、红绿验证和工作区基线。9组运行差分覆盖未绑定、初绑、替换、
缺失、Removed、连续命令顺序、Send超时删除、随后跳过、重绑恢复。额外 Rust 测试
覆盖1024队列饱和、单槽绑定背压、传输阻塞期间重绑、动态移除和不同store隔离。
Go对这些额外时序的依据是源码契约，未将其声称为新增Go运行差分。

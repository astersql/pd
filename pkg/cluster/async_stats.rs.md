# 异步统计与 witness 事件桥接

`handle_stats_async` 借用宿主已有 `HotCache` 与已安装
`TransferWitnessLeaderScheduler::recv_region_info()` 的 sender，接收一个不可变
`Arc<RegionInfo>`。不创建缓存、线程、Tokio runtime 或独立 heartbeat 管线。

顺序对应 Go `pkg/cluster/cluster.go::HandleStatsAsync`：先投递 write expiration、
read expiration、write flow，再用 `need_transfer_witness_leader` 检查单独的 leader
字段并非阻塞投递 witness。无 leader、普通 leader 或未安装 scheduler 均跳过。
不因热点队列 Full/Closed 提前返回；witness Full/Closed 也不回滚热点准入或 root。
`AsyncStatsReceipt.hot` 给出三个独立执行凭据；`witness` 是准入结果而非完成结果。
丢弃热点凭据不会撤销已准入工作。停止和 join 仍由各 canonical owner 负责。

每次 heartbeat 均独立投递，不去重、不从 root 再读另一版本：Go 此入口直接转发参数，
`Collect` 才重新读取最新 root。StatePipeline 在原有准入后把 receipt 的同一个 Arc
传给本桥接；已准入重复报告仍带流量，过期报告不进入通道。队列保留当时视图，
后续同 ID root 更新不替换已排队事件；调度消费时继续由 canonical scheduler
检查当前 store 安全性。

Go controller 的 full 分支是 best effort；Rust 同时显式暴露 receiver 关闭结果供宿主
诊断。此桥接不写持久化，无应当注入的存储 I/O；热点停止、两个队列独立背压、
receiver 丢失及真实 scheduler 消费由独立 task_127_test.rs 验证。
Go 依据为只读源码推导，未声称运行差分，详见
[任务127证据](../../docs/rust/schedule-execution/task-127.md)。

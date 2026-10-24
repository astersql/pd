# 热点任务执行与关闭

`HotCache` 使用宿主提供的 Tokio `Handle` 启动 read/write 两个独立 FIFO worker，
不创建 runtime、全局订阅器或协议依赖。容量由宿主指定且必须大于零。
`HotTask` 拥有 metadata、leader store、load 数组和已报告 region 集合；
`new_region_flow_tasks` 仅从 RegionInfo 捕获 meta、leader store、wrapping 秒差及
write bytes/keys/query。`RegionInfo::report_interval` 仅增加已有领域值的只读访问器。
每个 worker 独占真实 HotPeerCache；`HotPeerCache::run_task` 是相同命令的同步入口。

`check_region_flow_async` 按 Go 顺序提交 write expiration、read expiration、write flow。
三个 admission 结果分别返回，不能把部分成功解释为完整 heartbeat 已应用。
单队列顺序稳定，跨方向没有全局快照原子性；过期先于本次 write，与 Go 任务顺序相同。
Read 使用显式单 store；Cold 使用 store-local reported region 集合；运行成功后才应用动作。
宿主在 admission 时传入同一时基的纳秒时间；报告 interval 使用整秒。
GC/TTL 策略沿用既有缓存，由宿主显式驱动，本文件不引入存储持久化。

`submit` 的 Ok 只说明入队，Full/Closed 是拒绝；`HotTaskReceipt::wait` 的 Applied
才说明命令已执行（低流量/空 peers 可能没有更新动作）。读写类型错误返回 WrongDirection。
Receipt 的 cancel 用原子 pending→canceled 转换，成功保证任务不会执行；已经 claimed 的
任务不能取消，可完成并报告真实结果。丢弃 receipt 不取消任务；丢弃 wait future 也不撤销任务。
Worker 不在计算中 await；取消不会造成一个任务中部分 actions 应用。

`cancel` 拒绝后续 admission，worker 关闭 receiver 并将未执行任务回复 Canceled；
已经 claimed 的任务允许完成。`shutdown` 必须等待完成，取消并 join 两个 worker（即使一个失败），
返回最终 read/write cache；重复调用返回 None。宿主 runtime 必须保持运行。
不要中途丢弃 shutdown future；Drop 只请求取消，不提供 join 完成保证。
宿主销毁 runtime 或 worker 异常时，receipt/关闭返回 WorkerStopped，不误报 Applied。

Peer/Stats 查询返回 HotPeerStat::clone 的独立去噪快照，不泄露滚动窗口。
Stats 与 Go 同样筛选 degree>=min、非 inCold、anti==方向默认值，按 store/region 升序稳定返回；
空 store tombstone 不保留。IsHot 只检查当前 metadata 的 peer membership 和 degree，
不套用 Stats 的 anti/inCold 条件；两方向任何一个 true 即 hot，但查询错误仍返回错误。

独立 `hot_worker_test.rs` 显式注册，覆盖 Go oracle 的 0/1/2/3/5/9 peers、read/write
各100次 IsHot、字段捕获、完整缓存/窗口状态与同步路径相等、48条排队读/冷/过期事件、
TopN/TTL 后果、容量/部分拒绝、取消、runtime 丢失、Drop、共享多线程宿主及重复 join。
Go oracle、红绿日志和 workspace 检查见 [任务23证据](../../docs/rust/state/task-23.md)。

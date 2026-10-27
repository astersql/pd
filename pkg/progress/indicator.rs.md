# Progress 数值与速度窗口

`Progress` 是独立领域快照，action 保留 Go 的开放字符串类型；
`ProgressIndicator::new` 接受当前累计工作量、目标及固定采样间隔。
`push` 使用系统时间，`push_at` 允许宿主在完成时间边界注入时间；采样速度始终
按样本数乘 update interval 计算，与墙钟经过时间无关。

对照只读 `indicator.go`：保留最多 `2h/interval+1` 个样本，初始 active window
为 `10m/interval`；patrol 调整将 duration 限制到 10m–2h，再加一个端点样本。
调整仅在下一次 push 移动 front；VecDeque 中的 active sample count 等价于 Go
双向链表的 front/currentWindowLength，扩窗时能够重新使用仍保留的历史。

负速度归零；百分比只作上界截断；速度先使用未截断样本计算。ETA 遇 NaN/Inf
转为 f64::MAX，包括完成且速度为零的 0/0。零目标初始化仍保留 IEEE NaN/Inf，
第一次 push 才设为 100%、零速度、零 ETA。完成时间可以被后续完成样本刷新；
进度回退或目标增大不清空已有完成时间。`raise_target` 保留 preparing 目标只增语义。
移除节点的“当前剩余 size → 累计删除 size”转换属于后续 manager，本文件输入为累计量。

零间隔在 Go 中会除零 panic；Rust 用 `InvalidUpdateInterval` 返回同一非法输入的
失败结果，避免领域库 panic。Rust Duration 无负值。过大间隔按 Go 限制到 10m。
没有引入后台任务、全局时钟、transport、存储或 runtime 依赖。

来源推导、11组固定期望与3项真实 Rust 测试见
[任务证据](../../docs/rust/schedule-execution/task-130.md)。没有 Go 运行差分声明。

任务131进一步冻结动态窗口边界与连续性，复用此 canonical 实现，不新增重复
`indicator_window.rs`。见[动态窗口证据](../../docs/rust/schedule-execution/task-131.md)：
12组时长/间隔边界、保留历史的缩窗再扩窗、样本不足、完成后调整与回退，
以及缺少端点样本的错误实现检测。窗口改变不立即刷新快照/完成时间；下一次
push才用新窗口重算，缩窗只移动active front，不丢弃容量范围内历史。

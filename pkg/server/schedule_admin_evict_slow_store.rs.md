# 慢节点驱逐配置 HTTP 适配

`serve` 接受宿主独占借用的 canonical `EvictDiskScheduler` 与
`EvictNetworkScheduler`、StorageEndpoint、ClusterPaths、宿主实际 disabled 值和相对 HTTP 请求。
仅处理 POST `/config`、GET `/list`；未知路径404明文，方法不符405空body。
宿主负责锁、路由挂载与 runtime；此处不增加完整 server/api 装配。

Go `evictSlowStoreScheduler.ServeHTTP` 委托专属 handler，故不使用 BaseScheduler
的 `not implements` 响应。按只读 Go 源码推导，恢复时长先校验，随后 batch，
最后 network enable；缺失/null/未知字段保留原值但仍持久化。batch范围1–100
且必须整数；恢复时长允许非负浮点截断。list输出六个策略字段及固定 false 的 disabled，不泄漏内部时钟。
保存完整 envelope 保留宿主 disabled（请求中的 disabled 不更新它）。
JSON错误400/input，流读取失败500/internal，保存失败500/JSON字符串。
JSON仅归一化对象顺序、缩进和尾随换行；真实 Rust I/O 错误、栈不伪造为Go。

`update_with_disk` 在已有领域策略间同步当前 disk 快照，成功持久化完整 envelope
后仅修改 disk 数值策略，保留磁盘驱逐及恢复时钟、网络恢复表及引用。
保存失败回滚数值策略，network enable 保留新值，与 Go persistLocked 一致。
这两个 owner 必须由宿主串行驱动；磁盘/网络 observe 的独立持久化和完整生命周期
组合仍沿用既有领域边界，本文件不宣称新增统一调度循环。

按用户明确授权采用源码/测试推导与 Rust 真实行为和故障验证；无 Go 运行差分。
未声称覆盖超出 uint64 可表示范围的浮点转换（Go语言对此不保证跨平台结果）、
任意损坏JSON的全部解析器诊断、Gorilla重定向/路径清理。
测试、来源哈希、红绿和全量验证见 [任务141证据](../../docs/rust/schedule-execution/task-141.md)。

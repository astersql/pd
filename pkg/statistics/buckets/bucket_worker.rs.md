# 桶任务队列与取消

`BucketWorker::start` 接收宿主 Tokio `Handle`、父取消令牌、宿主 `Registry` 和独占
`BucketRangeIndex`。库不创建runtime，不安装全局指标、信号或订阅器，也不依赖协议与存储。
`BucketWorkerClient` 可克隆，报告与查询共用容量20000的FIFO队列；并发生产者顺序以实际
入队顺序为准。`check_async`/`collect`不等待容量，拒绝返回`Full`或`Closed`且无索引效果。
报告输入必须满足既有转换器的有序键/计数长度前置条件；无效报告panic会由join报告。

worker独占索引：同region同范围用原region继承，否则用有序重叠项继承，再计算冷热，
最后put。查询按min_degree和region_ids返回独立快照。`BucketStatsWait::wait`只消费一次，
等待者取消返回`Cancelled`，worker退出返回`WorkerStopped`；空结果仍是`Ok(empty)`。
等待取消或丢弃不撤销已接受任务，oneshot发送不阻塞，后续报告/查询继续执行。

worker使用父令牌的child，关闭不会取消宿主。循环在每次取任务前优先检查取消；已经开始
的同步任务会完成，待执行任务被丢弃。查询等待优先级为调用者取消、worker停止、结果，
因此取消与已就绪结果并存时有明确错误。并发提交与取消竞争时可能刚好入队，但接受不代表
必然执行，关闭后观察到取消的提交一定返回Closed。此确定性关闭是对Go随机select/no-join
生命周期的显式细化，不宣称为Go全部关闭竞争路径的运行差分。

`shutdown(&mut self)`取消并join，重复调用安全，等待future被丢弃后仍可再次join。
worker panic取消等待者并将JoinError返回宿主。Drop仅请求取消；宿主应显式shutdown以
确认资源退出。调用方持有的client不能维持后台任务存活。Registry指标保留至宿主释放，
同registry重复注册明确返回错误且不spawn；宿主为worker生命周期管理该registry。

指标兼容`metric.go`：`pd_scheduler_bucket_task_duration` HistogramVec，type固定两值
`check_buckets`/`collect_bucket_stats`，help原文不变，23个0.0001×2^i边界，耗时秒。
两个label handle在worker开始时缓存，不在每个任务上查找标签；只对完成的任务observe。
固定标签不含region/id等动态值，无持续增长/清理问题。宿主Registry注册失败通过Result
上报，遵从library-first要求，不使用Go包级MustRegister/global init。

测试显式注册于lib.rs的独立`bucket_worker_test.rs`，覆盖既有Go两个测试输入、FIFO、
查询快照、满队列拒绝及恢复、等待取消/丢弃、父取消、shutdown/drop/join、panic回收、
取消shutdown future再join、重复注册，以及同一宿主上多生产者和宿主继续运行。
证据见[任务8](../../../docs/rust/state/task-08.md)。

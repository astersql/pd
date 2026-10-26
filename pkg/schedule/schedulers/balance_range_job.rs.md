# Balance range 作业配置

`BalanceRangeJob`、`JobStatus`、`JobTime` 和 `BalanceRangeJobs` 属于
`astersql-pd-schedule-schedulers` 的配置领域；调度求解、operator、HTTP 路由和宿主
runtime 留给其它任务。`from_http` 只接收 JSON 参数，返回类型化领域错误，不处理 HTTP。

来源为只读 `balance_range.go` 的 handler、config、job、JobStatus，`init.go` 的
slice decoder，以及 `utils.go`/`keyutil` 的范围解码。采用用户授权的源码推导和 Rust
行为/故障验证；没有 Go 运行 oracle，也不声称运行差分。见
[证据](../../../docs/rust/schedule-execution/task-104.md)。

队列复用 canonical `SchedulerConfig<Vec<BalanceRangeJob>>`、`StorageEndpoint` 和
MetaStore，持久键是 `balance-range-scheduler`。独占可变访问跨越检查、保存、发布；
保存失败或 future 被取消时保持原本地值。宿主不得将调用失败解释为后端绝对未写入：
提交后响应丢失的通用 I/O 不确定性仍由 MetaStore 契约负责，reload 恢复持久快照。
多实例沿用现有 last-writer-wins，无私有 CAS/全局缓存。

add 重置 pending、仅拒绝未完成同 alias；完成后允许复用 alias。API 首 ID 为 1，
工厂初 ID 为 0；清空后重新从 1 开始，溢出沿用 uint64 wrapping。
删除代表 cancelled，保留既有 start，否则同刻设置 start/finish。begin/finish 验证状态；
GC 仅裁去连续 completed 且 finish 严格超过七天的队首，不跨过未到期/未完成作业。
peek 跳过 finished/cancelled；timeout 比较也使用严格大于。

默认 timeout 为 30 分钟；HTTP alias 必填，slice alias 可为空。复用 canonical Go
Duration 解析；范围解码保持 unmatched 尾参数忽略、空范围全域和 HTTP 两次 URL
unescape，包括非 UTF-8 字节。`Rule` 使用已有 core 枚举，持久化 KeyRange 使用已有
hex JSON 编解码，避免产生另一份字节范围序列化实现。

JobStatus 按原始 JSON token 精确匹配，未知 token 保留当前状态；反序列化到新对象时
保留 pending。未知整数状态序列化为 unknown。`raw_value` 仅用于保留 JSON 转义形式，
不是可选业务 feature。job timeout 保存为纳秒整数；create/start/finish 保存 RFC3339Nano，
缺省 create 是 Go 零时间 year 1，start/finish 缺省省略。时间领域表示为 signed Unix
nanoseconds；固定测试时间和持久化规范化到 UTC，不比较原时区偏移文本或 monotonic
元数据。显式时钟来自宿主，不创建 runtime 或读取全局时钟。

测试独立登记于 `task_104_test.rs`。真实 MemoryStore 执行所有正常读写，仅 MetaStore
read/transact I/O 边界注入 unavailable/stall；验证完整本地/持久快照和重建。来源
`TestPersistFail` 需要 failpoints，本任务未执行、注入或修改 Go。

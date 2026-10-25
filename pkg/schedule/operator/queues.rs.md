# 等待队列与推送队列

`queues.rs` 是 operator crate 的 canonical 队列实现，只持有真实
`Arc<Operator>`，不创建 runtime、后台线程、网络或存储对象。

- `RandBuckets` 以单个 mutex 串行化插入、快照和取出。非空优先级桶权重为
  1/4/9/16，按 low → urgent 累积比例选择，桶内 FIFO；权重不乘队列长度。
  最后一个元素取出后移除该桶权重。优先级在插入时确定。
- 宿主为 `get_operator` 传入 `[0,1)` 的随机样本。保留 Go 的浮点累积与
  半开区间规则，不额外制造确定性优先级。测试固定样本，不声称复刻 Go RNG。
- `put_merge_operators` 对长度非 2 或任一 related-region 信息为空的输入返回
  `Ok(false)`，不入队。两项放进第一项的优先级桶；不额外校验 kind、相互 ID
  或第二项优先级。出队按第一项的 related-region 元数据决定一次取出两项，
  同时覆盖 merge 与 affinity。调用者应在入队前完成 Sync，并成对提交。
- `list_operator` 返回独立的 Arc 快照，按桶及 FIFO 排列。对象状态仍由 canonical
  Operator 管理，不复制或冻结状态；快照在队列出队后仍然有效。
- `WaitingOperatorStatus` 以字符串为键，缺失值为 0；加减使用 wrapping 运算，
  保留 Go `uint64` 的溢出、下溢语义。计数与队列是不同的锁及独立调用，事务性
  的队列计数策略属于上层 Controller。
- `ConcurrentHeapOpQueue` 用单个 mutex 保护 BinaryHeap；反转比较获取最早
  deadline。deadline 是宿主同一单调时钟域的 `Instant`，不涉及 wall-clock
  序列化。相同 deadline 的顺序未定义，与 Go 的严格 `Before` 比较契约一致。
  `pop` 不检查是否到期，未来项也会取出；等待或重新插入由调用者负责。

`QueueError` 为无效优先级、非有限或越界随机样本、缺失配对提供类型化错误，
错误不消耗队列。前两项对应 Go 索引 panic／RNG 不可能输出的边界；第三项对应
Go 对不完整 related 队列的越界 panic。合法业务输入保持来源契约，不把 panic
作为服务控制流。内部 mutex poisoning 仍代表程序错误，不隐藏 panic。

只读来源为 `waiting_operator.go`、`operator_queue.go` 和
`waiting_operator_test.go`，以及 canonical `AdditionalInfo` 的非空配对判断。
[fixture](../../../docs/rust/schedule-execution/fixtures/task_033.json) 的期望来自
源码契约；依用户授权没有运行 Go，也没有宣称运行差分。

测试在独立 `task_033_test.rs` 中显式注册，覆盖固定权重区间、动态权重移除、
FIFO、快照、合法/非法配对、计数回绕、同 deadline、未来 deadline 和并发
生产/消费。权重变异会令同一行为测试失败；完整命令、数量、槽位和证据见
[task-033](../../../docs/rust/schedule-execution/task-033.md)。

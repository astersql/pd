# 任务033：等待队列与推送队列

生产路径 [queues.rs](../../../pkg/schedule/operator/queues.rs) 提供 `RandBuckets`
的单项/配对插入、加权取出、快照，`WaitingOperatorStatus` 的独立计数，以及
`ConcurrentHeapOpQueue` 的并发最早 deadline 堆。领域边界、错误与时钟约束见
[queues.rs.md](../../../pkg/schedule/operator/queues.rs.md)。仅增加 lib 模块/测试
注册，没有修改 Cargo.toml、Cargo.lock 或前序 Operator 实现。

## 依赖与来源

批次32实际提交 `0eaf960bc`，其 [task-032](task-032.md) 记录42个工厂场景、
52个包测试及1415个 workspace 测试。已核实 canonical Operator 的优先级、
`sync`、additional-info API 和真实 RangeOperatorFactory 配对产物；本任务复用
同一 Operator 与 AdditionalInfo，不另建操作模型。oracle-index.json 中既有
运行记录不覆盖任务033队列；未借用其它任务运行结果充当本任务 oracle。

[fixture](fixtures/task_033.json) 冻结 Go 基准
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 的4份源码 SHA256、输入及预期输出。
已逐字节确认来源与该 commit 一致。证据为 `source_assertion`、
`go_execution=false`；用户已解除 Go 运行 oracle 准入限制，本任务未运行 Go，
未宣称 Go/Rust 运行差分。fixture SHA256：
`285f4815fb6f8e32143fe38234772b923440cd7a27a1d9c098132673e74fcbb4`。

只读对照入口及实际覆盖：

- `TestRandBuckets`：四种优先级全部取出及空队列。
- `TestListOperator`：四项快照数量；Rust 增加桶顺序、FIFO 和快照所有权断言。
- `TestRandomBucketsWithMergeRegion`：配对和普通操作混合、一次取出一项/两项。
- `TestPutMergeOperators`：merge/affinity 均按 related-region 元数据成对取出，
  无元数据配对拒绝、混合与空队列。
- `operator_queue.go`：严格 deadline Before 的最小堆、空队列和互斥；Go 上述
  waiting 测试不覆盖 heap 或并发。Rust 对这些源码契约单独验证。
- `waitingOperatorStatus` 的缺失计数0及 uint64 回绕由源码推导；Rust 直接验证
  下溢再加回0，而非自行改成饱和计数。

随机样本为宿主显式边界，按源码1/4/9/16权重计算预期，未从待测实现生成。
同 deadline 只对比 deadline 序列及区域 ID 集合，因为 Go 没有稳定顺序保证；
没有忽略取出错误、缺失或重复项。宿主 monotonic Instant 的基点只用于测试，
fixture 比较相对纳秒数，不对日历时间或 RNG 种子做一致性声明。

## 行为与故障验证

[task_033_test.rs](../../../pkg/schedule/operator/task_033_test.rs) 显式注册5个测试：

- `task_033_behavior_cases`：10个表驱动固定场景，10成功、0失败，覆盖半开权重
  边界、空桶重新归一化、权重不随桶长度变化、FIFO、空队列；另有快照顺序和
  动态移除桶权重的完整取出断言。
- `task_033_merge_and_faults`：merge/affinity 混合取出、第一项优先级选桶、
  长度0/1/3与缺元数据拒绝；非法优先级-1/4、NaN/Infinity/越界随机值和缺失
  partner 返回错误且不消耗队列，补入 partner 后恢复。Go panic/不可能 RNG
  输出的非法边界使用 Rust 类型化错误；合法输入保持来源语义。
- `task_033_counts`：独立键、增减、uint64 下溢及上溢回绕。
- `task_033_heap`：空队列、len、乱序及同 deadline、无重复/丢失、未来项照常
  取出；堆本身不承担到期等待。
- `task_033_concurrent_push_pop`：4个生产者与4个消费者在 Barrier 后共同运行，
  验证400个merge/affinity批次始终原子配对、800个堆项恰好一次交付，最终两个
  队列为空、独立计数为0；全部线程 join。无业务 mock、无持久化路径。

[missing-api.out](logs/task_033/missing-api.out) 保存首次缺失 queues API 的编译
失败，退出101，不计行为失败。
[red.out](logs/task_033/red.out) 保存将权重故意改为1/1/1/1的行为变异：同一
`task_033_behavior_cases` 在 `medium-start` 断言失败（实际区域1、预期区域2），
4通过、1失败、退出101。之后恢复正确权重并重新格式化、验证；错误实现未保留。

## 执行与只读边界

所有 Cargo 命令均通过 [slot-run.py](logs/task_033/slot-run.py) 原子动态领取
仓库共享槽位1，实际
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁记录计划、任务、会话及PID；正常、失败和信号路径先等待子进程再释放自己的
锁，保留编译缓存。[validation.json](logs/task_033/validation.json) 保存确切
命令、退出码和有效测试数量，各命令完整输出为相邻 `validation-N.out`。

workspace 使用既有 etcd 外部 I/O fixture。
[readonly-validation.py](logs/task_033/readonly-validation.py) 及
[Go 拦截器](logs/task_033/reuse-go/go) 只允许复制已存在的二进制，拒绝真实 Go
编译或其它 Go 命令。[provenance](logs/task_033/fixture-reuse-provenance.json)
保存来源二进制和源码哈希，[go-reuse.jsonl](logs/task_033/go-reuse.jsonl) 保存
复制事件。该二进制只服务既有外部 I/O 测试，不是本任务 Go oracle。

[verification.json](logs/task_033/verification.json) 保存1101份 Go/模块文件的
前后哈希一致证据、空 Go Git 状态、4份来源源码匹配基准、两个只读 plan.md
完全未变和 fixture 哈希。未运行 make/failpoint/Go格式化等写入入口。

## 最终结果

状态：已完成。全部必需命令退出0：

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_033 -- --nocapture` | 0 | 5通过，10个固定场景及400个并发批次 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 57通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1420通过，0失败，1既有忽略 |

`git diff --check` 与 Markdown 相对链接检查通过。已审查范围仅为 queues
生产实现、独立测试、lib 注册、同名说明、fixture 与本任务证据；其它任务的
既有文件删除未纳入提交。所有验证子进程已退出，自有槽位1锁已释放，缓存保留。
使用 git-commit 技能仅提交这些实际变更，任务033完成后按授权删除编号任务文件。

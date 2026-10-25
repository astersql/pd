# 任务036：心跳派发与陈旧操作处理

状态：已完成。

生产路径 [controller_dispatch.rs](../../../pkg/schedule/operator/controller_dispatch.rs)，
领域边界见 [controller_dispatch.rs.md](../../../pkg/schedule/operator/controller_dispatch.rs.md)，
测试 [task_036_test.rs](../../../pkg/schedule/operator/task_036_test.rs)。
复用 ControllerAdmission 的真实状态、既有 steps/限流 Ack、heartbeat command/router，
没有复制 operator map、store limit 或 transport。准入层仅抽出同 crate 可见的
`detach_operator`，使成功移除不引入虚构 cancel reason。没有改 Cargo 依赖或锁文件。

## 依赖及来源

已核实批次35提交 `89fd728ce` 及 [task-035.md](task-035.md)：10个聚焦、75个
包级、1438个 workspace 测试通过。当前限流、影响查询、fast influence、push heap、
身份比较移除和 OpStep API 可用；本任务重新验证包级和 workspace。

已读取 oracle-index.json。现有 operator_concurrent_replace 只覆盖其记录的并发故事，
不能代表本任务全部派发场景。用户已解除 Go 运行 oracle 准入限制，本任务冻结
**源码推导期望**：`source_assertion`、`go_execution=false`，没有运行 Go，也不声称
Go 运行差分。[fixture](fixtures/task_036.json) 记录来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、controller 源码和测试 SHA256；
验证脚本核对当前字节与该 commit 一致。

只读采用的 Go 契约：

- Dispatch：先 Check，再按 STARTED/SUCCESS/TIMEOUT 分支；只有 heartbeat 检查 stale。
- checkStaleOperator：步骤 CheckInProgress 在 confver 检查之前；Unhealthy 权限仅
  交给 TransferLeader，Down 检查保留。confver 差值按 unsigned wrapping，与当前及
  已完成步骤的 ConfVerChanged 比较。成功优先，不对成功操作再次拒绝 epoch。
- SendScheduleCommand：调用步骤 GetCmd，nil 命令/缺少 leader 不发送；V2 由宿主
  协商结果传入；领域 envelope 保留 cluster/region/epoch/leader 与动作字段。
- getNextPushOperatorTime/pollNeedDispatchRegion/checkOperatorLightly：快步骤2秒、
  其余5秒；到期前先推进完成步骤、检查 region 丢失和关联 merge 的 version 增长。
  队列旧条目按 region ID 解析当前 replacement；未来 deadline 重新入堆后停止轮询。
- TestPollDispatchRegion、TestPollDispatchRegionForMergeRegion、
  TestDispatchOutdatedRegion、TestDispatchUnfinishedStep 提供场景契约。
  TestConcurrentAddOperatorAndSetStoreLimit 的既有 Rust 对应测试随包级回归运行。

## 行为、故障与宿主边界

`ControllerDispatch::{dispatch,send_schedule_command,schedule,poll_due,push_due}`
使用真实集群、操作及路由。`OpStep::{get_cmd,check_in_progress}` 穷尽12种步骤，
委托其既有实现；统一保留类型化 Leader/Peer/Remove/Joint 错误。

聚焦10个测试包括 fixture 的7组场景（7成功/0失败）：current、stale、older-wrap、
create-bypass、notifier-bypass、success-before-stale、no-leader。其余验证包含：

- 步骤推进后发送 RemovePeer，完整命令 leader/epoch/目标/动作字段；成功移除和
  小于10秒的真实 fast influence；较慢成功不进入 fast cache；nonwitness TTL 效果。
- pending learner 消耗恰好一个 confver 预算但不重复下发，pending 清除后推进。
- 健康权限开关与非领导步骤的掩码；Down 无条件拒绝；缺少目标保留类型化错误。
- 真正 mpsc transport 断流后 SendFailed/unbind，重绑后按2秒 deadline 重试成功；
  3秒不重发、4秒重发、移除后清除残余 heap 条目。无调用次数 mock。
- 5秒慢步骤；replacement 使用旧 heap 条目的期限并调度新 operator；完成步骤在
  未来 deadline 前返回；merge 版本变化/region 丢失先于期限取消并移除关联成员。
- v1/v2 membership/witness 字段、joint leave 空 envelope、passive merge 和不支持
  V2 时的 command suppression；超时移除及 RoutingClosed 类型化传播。

只控制显式 Instant/Unix clock 和外部 I/O channel。健康阈值按真实配置设置，
不同 region 使用互不重叠范围，heap 需要确定顺序的测试使用不同期限。未对错误、
状态、成本、目标或 epoch 做归一化。本行为没有持久化路径。

宿主继续负责历史记录、回调和随机输入；DispatchOutcome 返回成功/nonwitness TTL
事件、removed 成员和 promote_waiting 标志，宿主用既有 promote_limited_waiting
执行晋升，并对新准入调用 schedule/Create 派发。poll 中 region 消失的移除按 Go
不请求晋升。连续 push 的 enqueue 错误会返回 RoutingClosed；需要逐个消费副作用
的宿主使用 poll_due/dispatch 事件接口。没有扩展到后续 facade/runtime 装配。

## 失败验证

[red.log](logs/task_036/red.log) 保存缺失 ControllerDispatch/DispatchSource API 的
编译失败（exit101）。随后接入真实 API 并修正测试构造；没有伪造运行结果。
[green.log](logs/task_036/green.log) 保留初期健康场景构造失败；默认配置的 Down
阈值为零，改用明确1800秒阈值并在601秒/1801秒验证 Unhealthy 与 Down 边界。

[mutation.log](logs/task_036/mutation.log) 保存同一10个测试对错误实现的实际失败：
临时关闭 confver stale 拒绝分支，fixture stale 场景断言 enqueued=false 失败，
9通过/1失败，exit101。finally 恢复源码，重新格式化后全部通过。此注入只改本任务
Rust 文件，不涉及 Go/failpoints。

## 验证记录

[run.py](logs/task_036/run.py) 每阶段依次原子 mkdir 领取共享槽位1–10，写入
plan/task/session/PID，设置绝对 CARGO_TARGET_DIR，子进程结束后 finally 释放自己
的锁并保留缓存；信号中断等待/终止自己的进程组后清理。所有阶段实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
确切命令、退出码在各同名 JSON，合并 stdout/stderr 在 log。日志只移除末尾多余
空行，记录原始哈希和移除数量，验证可重建原始字节。

## workspace 环境故障及处理

最终断言改为显式源码常量后的默认并发回归曾在4个既有 storage-endpoint etcd 测试
报 Unavailable（exit101）。完整日志见
[workspace-parallel-failure.log](logs/task_036/workspace-parallel-failure.log)。没有改写
这些测试或业务代码。以 RUST_TEST_THREADS=1 重跑时这4个用例均通过，但既有
`real_etcd_oracle_process_restart` 在解析子进程 `PROCESS_TSO` 行首标记时失败
（exit101）：单线程 libtest 将测试名前缀和子进程 stdout 接在同一行，导致
strip_prefix 找不到标记。见
[workspace-single-thread-failure.log](logs/task_036/workspace-single-thread-failure.log)。

改用 RUST_TEST_THREADS=2，独立运行
`cargo test -p astersql-pd-tso --locked --test etcd_behavior real_etcd_oracle_process_restart -- --nocapture`，
1通过/0失败，exit0，见 [tso-two-threads.log](logs/task_036/tso-two-threads.log)。
最终 workspace 采用同样的两线程环境，完整命令和环境变量写入结果 JSON；没有
跳过测试、降低断言或修改本任务外代码。默认并发的 Unavailable 原因只能判为
测试环境瞬态失败，不能将其当作本任务行为差异或声称已修复其根因。

## 最终结果

最终源码的所有必需检查退出0：

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_036 -- --nocapture` | 0 | 10通过，0失败，0忽略；fixture 7组全部通过 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 85通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1448通过，0失败，1既有忽略 |

故障注入9通过/1失败（exit101），恢复后10个聚焦测试通过。工作区既有 server
deprecated-field 警告不影响 check/test 退出码，唯一忽略来自既有测试。
命令、有效数量、来源/fixture/Rust 哈希及审计结果见
[verification.json](logs/task_036/verification.json)，可用
`python3 docs/rust/schedule-execution/logs/task_036/verify.py` 重新核对。

1101份 Go/模块文件哈希前后一致，Go Git 状态为空，两级只读 plan.md 未变。
fixture SHA256：`09305ce5eb81d4147eb0a8a3192a05ee865f9709a5a4f553c937fe8e9da5d6fc`。
全部阶段实际领取槽位1，绝对 CARGO_TARGET_DIR 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；相关进程已结束，自己的锁
已释放，缓存保留。Markdown 链接与 diff 检查通过；日志 EOF 归一化可重建原始哈希。

使用 git-commit 技能只提交本任务代码、测试和证据，排除其它任务既有删除。
任务编号文件获得验证证据后标记已完成，再在报告完成证据后删除，不单独提交
其状态更新或删除。没有修改原主题或两级只读 plan.md。

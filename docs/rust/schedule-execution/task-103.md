# 任务103：见证领导者转移调度

状态：已完成。

批次102真实依赖由提交 `aba8d8b69`、[task-102](task-102.md)、canonical core、
StoreStateFilter 和 PeerOperatorFactory 核实；前置包级156项、workspace1705项通过。
只实施任务103，无并行任务/子代理；整体及包级 plan.md 保持只读。

采用用户后续明确授权：只读 Go 源码/测试推导 + Rust 真实行为及故障验证，替代缺失的
Go 运行 oracle/差分准入。没有执行 Go、failpoint 或声称 Go 运行差分。
[provenance](logs/task_103/provenance.json) 固定 source_commit、计划基准、来源和 fixture
SHA-256；runtime_oracle=null。已审查 oracle-index，未套用无关历史运行输出。

## 实现及来源边界

[transfer_witness.rs](../../../pkg/schedule/schedulers/transfer_witness.rs) 提供
`TransferWitnessLeaderScheduler::{new,recv_region_info,is_schedule_allowed,schedule}`；
[模块说明](../../../pkg/schedule/schedulers/transfer_witness.rs.md) 解释队列与宿主边界。
lib 显式注册生产模块及独立测试，无新增 Cargo 依赖或锁文件变更。

只读 Go transfer_witness_leader.go 固定容量10000、batch3、始终允许调度、重复不去重、
错误消耗batch槽、不重试/不重新入队、pending/down按store排除、Urgent store filter、
随机主目标及完整备选、WITNESS_LEADER kind和Urgent优先级。core/region.go、store.go
固定follower及缺store语义，operator builder/create_operator.go固定角色/健康/放置验证。

Go TestTransferWitnessLeader 是限额耗尽/恢复和多目标heartbeat完成语义来源；
TestTransferWitnessLeaderWithUnhealthyPeer 是pending/down及消费后空队列来源。
该测试末尾pending+down分支没有重新enqueue，不能声称它实际覆盖了该事件；本任务
Rust显式enqueue坏事件并校验NoTarget。没有运行这两个Go测试。

Go没有要求本地检查witness leader资格（普通leader也可入队）；资格由上游协调器判断。
Go候选follower仍可能是witness/demoting voter，随机选中后builder拒绝；保持这个错误，
不新增角色预筛选或重试策略。builder备选列表沿用canonical契约，不改变其语义。

## 行为、故障与红灯

[fixture](fixtures/task_103.json) 固定14条源码推导案例：witness/普通leader、首/末随机目标、
pending、down、无健康peer、learner排除、目标store缺失/offline/disconnected、真实
TRANSFER_LEADER_IN预算耗尽、witness/demoting目标builder拒绝、缺leader错误。

归一化：Go map顺序未定义，按store ID排序；随机首/末索引为外部输入；固定region1、
peer/store1..3、clock=10^12ns。不忽略错误、kind、优先级、备选或状态差异。
适用mode为默认features的领域输入（classic/nextgen共享契约）；纯leader transfer
不包含成员变更步骤，无需构造conf-change-v2分支或新增feature。

- `task_103_behavior_cases`：14条；校验NoTarget/NoLeader/LeaderNotAllowed具体类型，
  单步LEADER|WITNESS_LEADER、Urgent、desc/region、from/to/alternatives；实际生成
  heartbeat主/备peer命令、校验缺store时in-progress失败、健康时成功、每个备选确认完成、
  confver=0、operator从start到success，原始region快照不被修改。消费后队列为空。
- `task_103_queue_batch_and_faults`：4条重复事件按3+1消费；坏事件加3条正常事件按
  1错误+2成功、下次1成功消费；入队后目标store消失时消费报错且不重试；10000条满队列
  try_send返回Full，消费后可再次入队；销毁consumer后producer返回Disconnected。
  is_schedule_allowed始终true，限额实际由store预算验证。没有只断言mock调用次数。

initial保存缺API编译红灯（exit101）和测试装配错误；focused修正RuleManager与
ProviderSnapshot具体类型后，focused2的2测试/14行为案例通过。
[red_behavior.py](logs/task_103/red_behavior.py) 暂将Urgent设置为Low，同一行为测试
priority断言失败，0通过/1失败、Cargo exit101；脚本确认失败后exit0，finally逐字节
恢复生产实现，SIGTERM/SIGINT也执行恢复。最终验证在恢复后重新格式化并运行。

无持久化配置或队列保存接口；本任务没有虚构MetaStore写入故事。真实MemoryStore只用于
canonical RuleManager装配；故障覆盖实际目标状态、预算、角色和队列边界。

## 最终验证

所有Cargo命令/脚本由[run.py](logs/task_103/run.py)原子mkdir动态领取共享槽位；
各阶段实际使用槽位1，CARGO_TARGET_DIR为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话/PID；正常/异常/中断先回收子进程再释放自己的锁，缓存保留。
完整确切命令和退出码保存在final.json，stdout/stderr保存于final.log.gz。
最终设置CARGO_BUILD_JOBS=1、RUST_TEST_THREADS=2。


| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 只保留本任务Rust变更 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_103 -- --nocapture | 0 | 2测试通过，14条fixture案例及队列故障 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 158通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | workspace元数据通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过；既有cpu_usage弃用warning |
| cargo test --workspace --locked | 0 | 1707通过/0失败/1既有忽略，含2文档测试 |

[final.json](logs/task_103/final.json) 的串行 `&&` 命令链全部exit0，
[完整日志](logs/task_103/final.log.gz) 与 [测试计数](logs/task_103/test-counts.json)可复核。
[audit](logs/task_103/audit.json)及起始hash表确认1103个Go/模块/只读plan文件内容
不变，无新增Go/模块文件、相关Git状态为空、Cargo.lock未变、git diff --check退出0。
所有阶段结束后自己的槽位锁已释放，缓存保留。文档相对链接、脚本语法和来源/fixture哈希
检查通过。仅提交本任务代码、测试、注册、说明和证据；保留其它编号任务既有删除，
完成后删除任务103编号文件，不修改任何plan.md或来源主题。

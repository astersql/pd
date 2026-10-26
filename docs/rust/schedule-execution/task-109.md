# 任务109：调度诊断结果聚合

依赖108实际提交为 `da4ccf41b`，日志压缩提交为 `9a685bcf4`；已审查
[任务108证据](task-108.md)（7个聚焦、179个包级、1728个workspace测试通过）。
本任务遵守批次109串行范围，整体/包级plan只读。

按用户后续授权使用 **只读Go源码推导 + Rust真实行为与故障验证** 替代本故事缺失的
Go运行oracle；没有运行Go测试、没有Go运行输出，不声称运行差分。oracle-index 中
已有运行案例没有覆盖诊断聚合。[provenance](logs/task_109/provenance.json)记录当前
源码commit、基准commit、来源文件SHA-256、fixture哈希及null运行输出。

生产路径：[diagnostic_record.rs](../../../pkg/schedule/schedulers/diagnostic_record.rs)，
[领域与兼容边界](../../../pkg/schedule/schedulers/diagnostic_record.rs.md)。
API：`DiagnosticRecorder::{new,is_allowed,analyze,set_result_from_status,
set_result_from_plans,get_last_result,len,is_empty}`、`DiagnosticResult` 和六个状态常量。
没有新增依赖、feature、Cargo.lock变更或跨包生产修改。

## 固定期望与行为

[fixture](fixtures/task_109.json)的四行输入为：空codes→normal/空summary，100+102→normal/
分别汇总两个store，403+403→pending/2个StoreDown，100+403→pending/分别汇总两个store。
每行对leader与region两类型各运行一次，共8个有效表驱动案例，通过8、失败0。
期望从Go diagnostic_recorder.go、balance_plan.go推导，在实现前冻结。

Go FIFO保留10条，取最新连续同status；weight_allocator将连续段按3个segment分配
3/2/1相对权重。Rust使用整数单位避免浮点并列舍入，等权重取较小(code,reason)，
汇总按(code,reason)排序。这是对Go map未指定顺序的确定化；并列情况下不声称与
任意一次Go执行返回相同的获胜状态。时间输入和查询时钟明确传入Unix秒，内部
StoreStatus不序列化。reason保留在分组键中，输出文字遵循Go Status.String。

5个真实测试：

- `task_109_behavior_cases`：上述8个案例、查询时间/name、4字段JSON、内部store map不暴露。
- `task_109_allow_status_and_faults`：18种内置类型及unknown、动态配置开关、六种状态和
  自定义状态、无store的pending reach limit、错误计划类型与部分汇总错误。
  Go丢弃summary error，Rust保留pending/空map语义。
- `task_109_history_weights_capacity_and_ownership`：三条数学等权重的稳定并列、较老
  历史被状态断点截断、10条上限、同秒重复记录、输入与查询结果修改不污染历史。
- `task_109_real_dry_run`：真实BasicCluster、RuleManager/MemoryStore、labeler、influence、
  BalanceLeaderScheduler；成功产生从store1出发的TransferLeader，有operator时优先
  scheduling（即使plans类型错误）；analyze不入队；三个Tombstone节点产生真实
  StoreRemoved计划并汇总为`3 store(s) StoreRemoved; `；leader/store计数/influence未变。
- `task_109_deepest_step_and_fault_atomicity`：最深步骤覆盖较浅步骤；缺少资源触发既有
  canonical计划panic，捕获后历史不变，下一次查询仍可用。panic hook文本属预期故障。

## 失败与修正

`initial`缺少Rust API，exit101，属于编译失败，不冒充行为红灯。最小实现后`green`
3个聚焦测试通过。expanded/expanded2修正测试的StorageEndpoint和TransferLeader API
误用（exit101）；expanded3补齐fixture的count策略（exit101）；expanded4发现成功
leader调度可有操作器但没有plans，删除错误测试假设，保留成功操作器证据；expanded5
使用Offline期望所有source失败不符合Go filter规则（Offline仍可迁出），按只读
filters.go:leaderSource/isRemoved改用三个Tombstone，expanded6的5测试通过。
这些日志全部保留，测试搭建错误不算已修复的生产缺陷。

实现后[行为变异](logs/task_109/mutations.json)分别破坏容量、状态断段、最新权重、
operator优先级、normal判定；每个Cargo命令exit101，均为真实运行断言失败，编译错误
不计入。脚本exit0，finally恢复生产源码。对应`red-*.log.gz`保留原始stdout/stderr。
这证明同一组测试能够识别错误实现，补充初次缺API编译红灯的局限。

## 最终验证

实际命令、退出码和测试统计在[commands](logs/task_109/commands.json)，阶段槽位记录在
[final](logs/task_109/final.json)，原始输出保存在`logs/task_109/final-0.log.gz`至
`final-6.log.gz`（按下面顺序）。所有调用继承共享槽位的绝对CARGO_TARGET_DIR：

```sh
cargo fmt --all
cargo test -p astersql-pd-schedule-schedulers --locked --lib task_109 -- --nocapture
cargo test -p astersql-pd-schedule-schedulers --locked
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

所有阶段用run.py从仓库根原子mkdir动态领取槽位，写owner.json，失败/中断时终止并
等待子进程后释放自有锁，保留缓存。本任务各阶段实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2`。没有任务专属target、没有绕过锁的Cargo调用。

[只读审计](logs/task_109/readonly-check.json)确认1101个Go/模块文件清单、内容哈希及
Git状态完全一致，两份只读plan哈希一致，git diff --check退出0。Cargo fmt只产生
本任务Rust文件的格式调整；Markdown相对链接已核实。提交排除既有其他任务的删除。

最终七条命令全部exit0：聚焦5通过/0失败（表驱动8/0），包级184通过/0失败，
workspace 1733通过/0失败、既有ignored=1（含2个compile-fail文档测试）。
全部子进程退出，自有槽位锁已释放，缓存保留。

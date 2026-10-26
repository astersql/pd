# 任务077：领导者授予调度

状态：已完成。

依赖批次76已满足：提交 `461af5c9e` 和 [task-076](task-076.md) 的真实配置事务、
core暂停及全仓验证证据已审查。读取两级只读计划、ADR、porting-guide、仓库规则及
oracle-index.json。按照用户对整个计划的后续授权，以 **Go只读源码/测试推导 + Rust
真实行为/故障验证** 作为验收；没有取得本任务Go运行oracle或运行差分，不伪造输出。

生产路径：[grant_leader.rs](../../../pkg/schedule/schedulers/grant_leader.rs)，
[领域边界](../../../pkg/schedule/schedulers/grant_leader.rs.md)。API为
`GrantLeaderConfig::new/stores`、`GrantLeaderScheduler::new/config/prepare/clean/`
`is_schedule_allowed/schedule`；lib.rs显式注册生产模块及独立测试。
共享context/result复用前序容器，业务仍使用canonical BasicCluster、follower index、
RegionPending/DownFilter、RuleManager及PeerOperatorFactory。
不增加依赖、features、私有runtime或持久化后端。配置HTTP/事务不在本任务范围。

## 来源及固定期望

基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`。
[provenance](logs/task_077/provenance.json)记录来源、fixture及Rust文件SHA256，
所读Go源码和基准一致，Go运行输出/hash明确为null。
[fixture](fixtures/task_077.json)冻结输入及源码推导期望，不从Rust输出反推。
Go TestGrantLeaderScheduler覆盖目标store3授予、OUT暂停和后续balance不再迁出；
扩展案例从grant_leader.go的Prepare/Clean、IsScheduleAllowed、Schedule及
create_operator.go/CreateForceTransferLeaderOperator和builder.go/allowLeader推导。
相邻Go测试依赖failpoints，未裸跑。

随机边界固定选择首个候选、不打乱；不声称Go随机顺序一致。store遍历用BTreeMap
稳定排序（Go map无定义顺序）。时间仅用固定heartbeat/now输入；没有忽略错误、
步骤或目标字段，没有额外结果归一化。classic/nextgen中该函数没有条件编译分支。

## 行为及故障验证

`task_077_behavior_cases`：20/20固定案例通过，覆盖正常、多目标、多ranges、完全
包含/不相交/部分相交、空ranges、已有leader、无follower、pending目标/其它peer、
down目标/其它peer、learner、witness、目标IN暂停时强制授予、Prepare后授予、
缺失store、每目标至多一个候选、空配置、多store Prepare/Clean。
多目标独立生成操作，可以为同一区域产生两个不同目标的操作，遵循Go而不去重。

逐项验证实际operator kind=LEADER、priority=HIGH、一步TransferLeader、目标集合、
from_store、空alternatives、confver变化0、source/target leader影响量-1/+1，
两种heartbeat命令目标、应用后RegionInfo的leader和operator成功终态。
构建失败保留类型化LeaderNotAllowed；不会创建假操作。
fixture开发时修正了从eviction测试沿用的去重断言，以及witness构建错误期望；
最终期望经Go allowLeader源码复核，未将这些测试设计修正算作生产缺陷修复。

`task_077_prepare_faults_and_limits`：真实缺失store99触发Prepare部分失败；成功store
暂停保留，Clean释放本次OUT引用并保留调用者既有OUT/IN暂停，最后独立恢复OUT。
验证限额2<3、3==3和0==0。无需mock业务结果或外部故障替身。
规则初始化使用真实MemoryStore/StorageEndpoint；此调度行为本身没有持久化写事务。

[red-api](logs/task_077/red-api.log.gz)：初次Grant API缺失，编译退出101，不作为行为失败。
[red-behavior](logs/task_077/red-behavior.log.gz)：临时把High改为Urgent，同一行为测试
在第一个实际操作优先级断言失败，1通过/1失败，退出101；恢复正确实现后再次格式化，
[focused](logs/task_077/focused.log.gz)最终2项通过，20/20案例通过，无新增忽略。

## 验证和审计

实际命令argv/环境/退出码逐项保存在[日志目录](logs/task_077/)同名JSON。
run.py每阶段使用原子mkdir动态领取共享槽位；所有阶段实际为 **槽位1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常、失败、中断路径均等待子进程退出再删除自己的owner/锁，保留编译缓存。
Rust修改后先运行`cargo fmt --all`。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_077 -- --nocapture` | 0 | 2通过，20/20固定案例及独立故障/限额测试 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 40通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1589通过、0失败、1既有忽略 |

[counts](logs/task_077/counts.json)从实际test result行汇总，含2项文档测试；唯一既有
忽略是tso_processes独立进程测试。原始stdout/stderr以gzip保存，
[编码说明](logs/task_077/log-encoding.json)包含压缩前SHA256。

[审计](logs/task_077/audit.json)：1101个Go/模块文件（含untracked扫描）清单、内容
SHA256和Git状态与开始一致；两份只读plan.md哈希不变。`git diff --check`退出0。
Cargo.toml/Cargo.lock不变。其余任务原有删除/修改不属于本任务提交。
实际槽位1锁已释放，编译缓存保留；生产实现、测试、证据和相对链接已审查。

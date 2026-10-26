# 任务079：慢磁盘节点驱逐恢复

状态：已完成。

依赖批次78由提交 `48d715a44` 和 [task-078](task-078.md) 核实；前置
canonical core/operator/storage API 可用。两份上级 plan.md 保持只读。
遵循用户对整个计划的补充授权，以 **Go只读源码/测试推导 + Rust真实行为与故障验证**
替代缺失的Go运行oracle，不宣称取得Go运行差分。已审阅oracle-index.json；
其中没有本任务所需磁盘状态机运行输出，按该授权使用明确标注的源码推导。

生产路径：[evict_disk.rs](../../../pkg/schedule/schedulers/evict_disk.rs)，
[领域说明](../../../pkg/schedule/schedulers/evict_disk.rs.md)。
API：`EvictDiskConfig::new/evict_store/ready_for_recovery`，
`EvictDiskScheduler::prepare/observe/schedule/clean/is_schedule_allowed`。
局部注册在lib.rs；复用唯一EvictLeaderScheduler批处理、真实BasicCluster、
StoreStats、RuleManager、StoreStateFilter、PeerOperatorFactory和StorageEndpoint。
仅为共享批处理增加内部description参数，既有evict-leader描述保持原值。
无新增Cargo依赖、feature或锁文件变更。

## 来源与边界

来源基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[provenance](logs/task_079/provenance.json)记录当前与基准各Go来源哈希且逐一相同，
以及固定fixture哈希；Go二进制、运行输出及其哈希为null。
[fixture](fixtures/task_079.json)是源码推导输入和期望，不能视为运行差分。
Go suite的TestEvictSlowStore、TestEvictSlowStoreBatch与TestRecoveryTime分别
支持检测/驱逐、批处理和恢复窗口；这些测试依赖failpoint，没有裸跑。
TestEvictSlowStoreBatchLimit的HTTP断言仅用于推导领域batch校验。
TestCalculateAvgScore是网络分支helper，本任务不实现或声称验证该分支。

检测中80分已计为slow、100分才触发驱逐；多于一个slow候选则不驱逐。
恢复从首次score<=1的观察开始，score>1会重置状态；已恢复标记在清理后仍保留，
再次捕获遵循Go现有时序。Prepare恢复持久化首个store，Clean清空配置并释放引用。
清理写失败时保留配置、仍释放cluster引用，错误通过类型传播；没有隐藏Go的该项行为。
Go missing-store分支在取GetID前检查nil较晚，Rust以Option安全进入同一清理意图，
不复制nil解引用风险。HTTP/network/metrics不属于该磁盘行为任务；领域JSON仅磁盘字段，
不能用它重写包含网络状态的完整配置。

时间使用显式单调Unix纳秒，elapsed按Go time.Duration上界饱和；runtime时间戳和
isRecovered不序列化。随机使用固定draw；完整目标集合比对且目标排序消除Go map顺序。
JSON对象顺序无语义；missing/null stores归为空，missing/zero batch恢复3。
没有归一化错误、操作步骤、优先级、影响量或持久化结果。

## 测试与故障证据

- `task_079_behavior_cases`：14/14冻结检测案例，涵盖0/79/80/99/100/101、
  第二慢节点79/80/100、不同store目标、serving/removing/removed状态。
- `task_079_recovery_window`：score 0/1/2/100、精确窗口边界、反复健康/变慢、
  recapture状态保留、allowed gate、清理后的真实引用状态。
- `task_079_persistence_faults_and_rebuild`：真实MemoryStore+StorageEndpoint，
  首次写失败回滚驱逐、恢复转换写失败回滚、Clean写失败保留配置但恢复cluster、
  持久化重建/Prepare/Clean、缺store错误、取消等待I/O回收新增引用。
  故障只注入MetaStore read/transact边界，正常存储操作使用已有后端。
- `task_079_config_boundaries`：1800秒/3默认、0/101/usize::MAX非法batch、
  JSON zero batch恢复、分数/负id/畸形JSON拒绝、超大恢复窗口。
- `task_079_real_operators_and_batches`：4/4批量1/3/5/100，109个真实操作，
  去重、LEADER kind、URGENT、desc、from/to候选、confver=0、leader影响量、
  两种heartbeat命令、Operator start/check/success及恢复后空操作。
- `task_079_removed_and_missing_recovery`：已驱逐store removed或missing时清理；
  另一owner的slow-store引用及IN暂停在磁盘清理后仍保留。

[red-api](logs/task_079/red-api.log.gz)记录API缺失编译错误，退出101；这不算行为红灯。
[red-behavior](logs/task_079/red-behavior.log.gz)临时将检测阈值100改为99，
同一固定案例对 `[99,0,0]` 的evict=0断言失败，5通过/1失败，退出101；
正确实现已恢复。[focused-final](logs/task_079/focused-final.log.gz)6通过/0失败。
首次故障测试曾共用重建/原实例存储而互相清理，之后隔离存储；新增operator测试
初次对StorageEndpoint调用clone不成立，[focused](logs/task_079/focused.log.gz)
保留编译诊断。两者属于测试设计修正，不宣称生产行为修复或额外红灯证据。

## 验证与审计

每阶段由[run.py](logs/task_079/run.py)使用原子mkdir动态领取共享槽位，记录task、
session和PID，命令继承绝对CARGO_TARGET_DIR。全部实际领取 **槽位1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常、失败及中断等待子进程结束后释放自己的锁；缓存保留。Rust修改后先cargo fmt --all。
每条确切命令、退出码、槽位及环境保存在同名JSON，stdout/stderr合并原样保存并gzip。

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_079 -- --nocapture` | 0 | 6通过，14检测案例、4批量案例及故障验证 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 52通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1601通过，0失败，1既有忽略 |

最终计数见[counts](logs/task_079/counts.json)，Go和只读计划审计见
[audit](logs/task_079/audit.json)。未修改Go源码/模块或两份上级计划。只提交任务79
相关生产、测试、说明和证据，保留任务78已有删除；完成后删除任务79编号文件。

审计确认1101个Go/模块文件（包含untracked清单）状态和内容哈希均保持不变；
槽位1锁已释放，缓存保留。全仓计数含2个文档测试，既有忽略为tso_processes。

# 任务082：停止节点驱逐恢复

前置81已核验：提交 `4bcff28bc` 与 [task-081](task-081.md) 的6个聚焦、62个包级、
1611个 workspace 测试证据及真实 storage/core/operator API 齐备。
按用户对整个计划的明确补充授权，采用 **只读 Go 源码/测试推导 + Rust 真实行为及故障验证**。
没有 Go 运行输出或运行差分；已读取 oracle-index.json，其已有路线不充当本任务运行证据。
整体、包级计划和来源主题均只读，未新增 Cargo 依赖或 feature。

生产：[evict_stopping.rs](../../../pkg/schedule/schedulers/evict_stopping.rs)；
[边界说明](../../../pkg/schedule/schedulers/evict_stopping.rs.md)。公开 API：
EvictStoppingConfig、EvictStoppingScheduler::{observe,schedule,prepare,clean,reload,update}、
EvictStoppingError。真实 leader 批处理沿用已验证 EvictLeaderScheduler。

## 来源和期望

来源基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[provenance](logs/task_082/provenance.json) 固定 Go 源码、测试及 fixture SHA-256，
两个 Go 文件均与基准一致。固定 [fixture](fixtures/task_082.json) 为0/1/2个stopping节点，
期望驱逐ID为0/1/0，手工从 Go 分支冻结，未从 Rust 输出生成。

对应 Go suite 的 TestEvictStoppingStore、TestEvictStoppingStoreRecovery、
TestRemovedStoppingStore 用于推导检测、真实 TransferLeader、恢复/Removed 行为。
额外配置、I/O故障、生命周期边界依据 Go persistLocked/PrepareConfig/CleanConfig/
ReloadConfig/updateConfig/scheduleStoppingStore 源码逐分支推导，不冒充 Go suite 已运行覆盖。
仅控制随机输入顺序；没有忽略错误、操作器字段或时钟输出。
领域实现无 classic/nextgen 分支，使用默认 Cargo features。

保留 Go 细节：同轮清理后不重新捕获；初始 Removing 不候选，但被选节点后来进入
Removing 且仍 stopping 则继续驱逐；Clean 写失败仍恢复引用而保留旧配置，
重试可以重复降低有符号引用计数。Reload 不重建引用，Prepare 才取得首ID引用。

## 测试驱动与案例

独立注册 [task_082_test.rs](../../../pkg/schedule/schedulers/task_082_test.rs)，真实
BasicCluster/StoreInfo/MemoryStore/StorageEndpoint/RuleManager/Operator，I/O wrapper
正常委托 MemoryStore，只在 read/transact 外部边界注入失败或暂停。

- task_082_behavior_cases：3/3固定检测案例，检查限额、core停止驱逐状态及恢复。
- task_082_lifecycle：missing/Removed/Removing 3种已选节点生命周期，和初始
  Removing/Removed/Preparing 3种候选边界，共6个有效表驱动案例。
- task_082_persist_reload_prepare_and_faults：真实存储往返、重建/Prepare/Clean、
  Reload零batch默认、missing store类型错误、JSON/ID/batch错误；捕获写失败/取消
  回滚引用；更新失败保留旧batch；清理失败保留配置但恢复引用；重试与实际存储
  字节解码状态；5.9截断为5和0/11/NaN无效更新。
- task_082_real_operators_and_batches：5/5 batch 1/3/5/10/100；先暂停所有目标
  验证无操作器且保留驱逐状态，再恢复目标，验证唯一region、URGENT/LEADER、
  description、单步TransferLeader、目标2/3、confver=0、leader influence、classic/v2
  heartbeat命令、应用后操作器成功完成，以及节点恢复后无操作器。

[red-api](logs/task_082/red-api.log.gz) 初始缺API编译失败（101），同时记录首版
测试常量缺失，后续已补齐；它不算行为红灯。
[red-behavior](logs/task_082/red-behavior.log.gz) 临时禁用候选检测，同一固定1节点
期望ID=1实际=0，4个测试全部失败（101）。恢复时曾因 rustfmt 改变文本使替换未命中，
pre-restore-* 保留该轮失败与主动中止workspace日志，未计入最终通过。
随后检查源码恢复正确谓词并完整重跑，最终聚焦4通过/0失败。

## 验证与共享槽位

[run.py](logs/task_082/run.py) 各阶段从仓库根原子mkdir动态领取共享槽位1–10，
记录task/session/PID，异常中止等待子进程结束才释放自身锁，缓存保留。
全部实际阶段均领取 **槽位1**，绝对 CARGO_TARGET_DIR：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
每条命令、退出码、槽位、并发环境见同名JSON，stdout/stderr合并后原样gzip保存。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_082 -- --nocapture` | 0 | 4通过，0失败/忽略；3检测、6生命周期、5操作器batch案例 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 66通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1615通过，0失败，1既有忽略 |
| `git diff --check` | 0 | 通过 |

[counts](logs/task_082/counts.json) 从真实stdout汇总，包含2个文档测试，既有忽略为tso_processes。
[audit](logs/task_082/audit.json) 确认1119个Go/模块/只读plan文件前后内容哈希相同，
没有新增Go或模块文件；两份指定plan.md和原始主题未改动。仅本任务Rust/测试/证据提交，
已有78/80/81任务删除保持原样并排除。槽位1自身锁已释放，编译缓存保留。
workspace测试复用既有只读rust-etcd-fixture Go源码构建，仅向所持target目录写二进制/WAL，
没有failpoint注入或改写Go源码/模块。文档相对链接已检查。

状态：已完成。

# 任务081：慢网络驱逐恢复状态机

依赖批次80已核验：提交 `c369b21b7`、[task-080](task-080.md) 提供已验证的
候选/图/评分过滤 API；批次79的真实磁盘配置与 leader 操作器 API 同样可用。
整体及包级 plan.md 保持只读。按用户对整个计划的明确补充授权，本任务采用
**只读 Go 源码/测试推导 + Rust 真实行为及故障验证**；没有 Go 运行输出或运行差分。
已阅读 oracle-index.json，其中其它任务运行证据没有充当本任务 oracle。

生产实现：[evict_network.rs](../../../pkg/schedule/schedulers/evict_network.rs)，
[领域边界](../../../pkg/schedule/schedulers/evict_network.rs.md)。公开 API：
`EvictNetworkConfig`、`EvictNetworkScheduler::{observe,schedule,prepare,clean,reload,update}`、
`EvictNetworkError`。唯一前置模块改动是 disk config 的 crate 内 `update_policy`，
保留其运行恢复时钟、状态及持久化磁盘 eviction，只更新经过验证的数值策略。
无新 Cargo 依赖、feature 或锁文件改动；网络状态不进入 server/runtime。
HTTP/Prometheus 适配留在领域模块外。

## 来源、固定输入和期望

Go 来源基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`。
[provenance](logs/task_081/provenance.json) 保留基准/当前源码及测试、
[fixture](fixtures/task_081.json) 的 SHA-256；两个 Go 文件均与基准相同。
fixture 的 trigger=0/5/6/7→evicted=false/false/true/true 手工从 Go >=6 推导；
其它时序/错误期望在独立 Rust 测试中直接冻结，没有从待测实现输出生成。

实际关联 Go 测试：`TestNetworkSlowStore`、`TestNetworkSlowStoreReachLimit`、
`TestNetworkSlowStoreSwitchEnableToDisable`、
`TestAddNetworkSlowStoreLockedSkipsGaugeOnPersistFailure`、
`TestNetworkNotConflictWithOtherScheduler`、`TestCalculateAvgScore`。
前五个属于需 failpoint 的 suite，未裸跑；avg helper 的只读断言用于边界推导。
任务原列 Batch/BatchLimit/RecoveryTime 主要是磁盘/批处理，不能当网络恢复 oracle。

Go 行为逐分支核对：恢复先过滤 tracked/removed 目标，再 wrapping u64 求和/整数平均；
均值<=1持续 recovery-duration 秒才能释放。首次健康只启动时钟，0秒仍需下一轮。
等待候选可在容量释放后激活，激活会重置其恢复时钟。六次触发且已暂停、未恢复才驱逐。
Prepare 重建 paused 的 runtime map，Clean 逐节点处理；Reload 保留 runtime map，
只调整旧/新 paused+disk 集合的 leader-in pause。网络/磁盘引用独立计数。

特意保留来源细节：Removing 在恢复阶段删除后，若旧评分仍慢可在检测阶段同轮再捕获；
检测容量使用阶段初始 paused 快照。禁用分支即使删除写失败仍清空 runtime map；
配置更新失败保留请求的新 enable flag，但 batch/recovery 数值回滚。
没有把这些来源行为静默修正。BTreeMap/按store ID排序固定 Go 未指定的 map 顺序；
多候选赢家选择没有 Go 确定性保证，不宣称随机顺序逐项相同。无其它字段归一化。

## 测试驱动与有效案例

测试文件 [task_081_test.rs](../../../pkg/schedule/schedulers/task_081_test.rs) 显式注册，
全部使用真实 BasicCluster/StoreInfo/MemoryStore/StorageEndpoint、规则与操作器。
只控制显式时钟、随机选择和 MetaStore 的外部 I/O 边界。

- `task_081_behavior_cases`：4/4 固定 trigger 案例，逐项验证 paused/evicted、
  core 引用、恢复时钟、窗口前1ns与精确到期、清理后的集合和允许 leader-in 状态。
- `task_081_waiting_activation_and_reset`：第二节点等待、tracked peer评分过滤、
  慢评分重置恢复、心跳缺评分均值0、首节点释放后激活第二节点、配置禁用、
  其它owner的 pause/evict 引用保留。
- `task_081_persist_reload_prepare_clean`：真实存储往返、时间戳未持久化、重建/Prepare、
  Clean、Reload pause改变但runtime map不重建、缺失store的2个类型错误、
  batch=0默认3、非法batch/JSON/负store ID、非法更新不发布。
- `task_081_removed_and_average_boundaries`：missing/Removed/Removing 3种生命周期，
  u64溢出、整数截断、均值1/2边界；Removing同轮再捕获明确断言。
- `task_081_io_faults_and_cancellation`：暂停写失败/取消回滚、驱逐写失败/取消回滚、
  config写失败的enable保留与数值回滚、Clean/恢复删除写失败保留状态/引用，
  backend恢复后重试成功，存储旧字节保持不变。I/O wrapper正常委托真实 MemoryStore。
- `task_081_real_operators_and_batches`：4/4 batch 1/3/5/100，验证真实
  URGENT/LEADER TransferLeader、唯一region、目标2/3、description、confver=0、
  leader influence、classic/v2 heartbeat命令，以及应用后成功完成与恢复后无操作器。

[red-api](logs/task_081/red-api.log.gz) 为新增测试缺API的初次编译失败（101），
不作为行为红灯。实现后的 [red-behavior](logs/task_081/red-behavior.log.gz) 将
驱逐条件临时从>=6改为>=7，同一固定6次触发用例得到false而非true，且真实操作器、
恢复/持久化/故障测试共5个失败、1个通过，退出101。随后恢复正确阈值。
[focused-final](logs/task_081/focused-final.log.gz) 6通过/0失败，包含4个固定输入及
4个操作器batch案例。全部初始编译/断言修复日志保留，不将这些失败计入最终通过。

## 验证与槽位

每阶段由 [run.py](logs/task_081/run.py) 从仓库根原子mkdir动态领取1–10共享槽位，
owner记录plan/task/session/PID；正常、失败或中断均等待子进程退出后释放自身锁。
缓存保留。实际使用过槽位 **1、2**：

- `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`
- `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`

这两个绝对路径为对应阶段的 CARGO_TARGET_DIR；最终聚焦/包级/workspace基线均为槽位1。
每条确切命令、环境、exit code及槽位见同名JSON；合并stdout/stderr原样gzip保留。
所有 Rust 改动后先 `cargo fmt --all`，审查只产生本任务相关变化。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_081 -- --nocapture` | 0 | 6通过/0失败/0忽略；4/4固定输入，4/4操作器batch |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 62通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1611通过/0失败，1既有忽略 |
| `git diff --check` | 0 | 通过 |

[counts](logs/task_081/counts.json) 从真实 stdout 汇总，包含2个文档测试；
既有忽略为 tso_processes。[audit](logs/task_081/audit.json) 确认1101个Go/模块文件
及17份只读计划（包含两份本任务指定plan.md）前后哈希相同，Go/模块Git状态为空，
未新增Go文件。两次最终核对结果一致；仅本任务Rust源码/测试、fixture、说明与日志纳入提交。
已有任务78/80删除保持原样，不纳入本任务提交。共享槽位1、2自身锁均已释放，缓存保留。
文档相对链接逐一核对；未更改整体/包级计划、来源主题或其它任务实现。

状态：已完成。

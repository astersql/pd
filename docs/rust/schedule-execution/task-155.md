# 任务155：可重复调度性能基线

状态：已完成。

前置154提交 `f93c03e31` 的七条最终验证命令均退出0，重新核实了全部日志哈希。
本任务只新增测试、target 和证据，复用既有生产算法：
`BalanceLeaderScheduler::schedule`、`BalanceRegionScheduler::schedule`、
`HotSolver::solve`。没有复制生产调度逻辑，没有新增 crate/feature/远程依赖；
仅将已有 canonical keyrange crate 声明为 compat 的直接 dev-dependency。

## 来源与边界

遵照用户后续明确授权，使用只读 Go 源码/测试推导与 Rust 真实行为、故障验证，
替代缺失 Go 运行 oracle 的准入；**没有执行 Go 测试，没有 Go 运行差分或 Go 耗时
实测，也不声称 Rust 比 Go 快**。oracle-index 原有分类与两份只读 plan 均未修改。
[source provenance](logs/task_155/provenance.json)固定来源 commit、逐文件SHA-256、
fixture、前置证据与 oracle-index 哈希；所有所用 Go 内容与来源提交相同。

逐项读取的 Go 基线范围：

- `balance_leader.go::createOperator`：先判断改善，产生 TransferLeader，附带
  sourceScore/targetScore 与 store health check。
- `balance_region.go::transferPeer`、
  `TestBalanceRegionOrdinaryMoveNotBlockedByCandidateSize` 和
  `TestBalanceRegionLargeCandidateDoesNotOvershoot`：普通大小区域的改善与防过冲、
  MovePeer 步骤、分数；它们不是本任务大规模快照的 Go 运行输出。
- `hot_region_solver.go::buildOperators/createOperator`、rank v1/v2 源码：
  比较候选后构造真实 operator，分配失败不返回半成品；热负载采用前置092已冻结的
  源码推导输入1000/500、peer200、两维rank4。复用该fixture不提升它的证据等级。
- `BenchmarkCandidateStores`：9999个 store 的候选排序和随机分数更新；不覆盖完整
  leader/region 调度。`BenchmarkNewBalanceReadSolvers`：1000个 store 的solver
  **构造**及ReportAllocs，不覆盖候选搜索。两者只能作为来源基准索引，未运行它们，
  不能拿名字或本任务耗时冒充 Go benchmark 数据。
- `placement/region_rule_cache.go`：候选/命中晋升及失效契约，供冷/热对照设计。

[冻结场景](fixtures/task_155.json)定义两种偏斜快照：4节点/1000区域、100节点/10000
区域，三副本均在1/2/3，初始leader都在1；其余节点是容量相同的空目标。单区域96MiB，
store容量6TiB；心跳时钟固定10^12ns，随机索引0、不打乱。hot候选区域leader为3，
source1→destination4；100节点、10000个真实区域/热peer候选。固定、合成的内存规模
场景使用真实组件；它不是线上TiKV网络集群或线上吞吐量的证明。

## 实现与案例

测试入口 [schedule_performance_test.rs](../../../tests/rust-compat/schedule_performance_test.rs)
在 compat Cargo.toml 显式注册为 `schedule_performance`；无需 lib.rs 重复注册。
真实 core/region树/store统计、placement RuleManager、MemoryStore/StorageEndpoint、
labeler、统计快照与 operator builder 均走 canonical 实现。

三个独立真实测试：

1. `task_155_behavior_cases`：两种规模 × leader/region × 冷/热 × 3采样，共24个
   有效测量案例。冷样本重建scheduler、显式失效所采区域placement缓存并确认未缓存；
   热样本先晋升真实fit缓存并用Arc身份确认命中，再额外执行不计入采样的预热轮。
   每次仅产生1个operator，检查所有步骤的重复一致性、region/target、分数、kind、
   learner→promote→remove及可能的leader转移、health check。冷/热结果完全一致。
   leader转移使计数差减少2，region移动使副本大小差减少192MiB；未把吞吐量或空循环
   当成调度。相同分数目标只允许在新快照重建时变化；同一快照所有采样严格比较结果。
2. `task_155_hot_solver_and_faults`：v1/v2 × 冷/热 × 3采样，共12个有效测量案例；
   每轮搜索10000个真实候选，验证rank4、operator数1、1→4的真实搬迁步骤。两维负载
   差都从500缩小到100。冷/热指solver对象新建/保留，加一次预热；HotSolver本身没有
   placement缓存，不把对象复用称为实际cache命中。两种公式各检查分配I/O边界失败
   （无operator、有错误）和空候选（无operator、无错误），共4个错误/边界案例。
3. `task_155_leader_convergence`：1000区域逐轮执行真实leader调度，宿主边界应用
   TransferLeader到真实region树和store统计。每次检查平方负载之和严格下降；计数守恒。
   666次转移后不再产生operator，最终 `[334,333,333]`；不是只计算理论改善。
   region/hot基线验证单轮结果及投影改善；不声称它们也完成多轮闭环。

每个独立测量进程记录36条样本，最终两次运行合计72条；另有6次不计入测量的预热。
[performance.json](logs/task_155/performance.json)保存原始数据、12组min/median/max、
两次进程运行按冻结fixture声明的等分目标归一化后的质量比较及收敛输出。
跨进程审计起初按原始步骤字节比较而失败：100节点region分别选中空目标10和46。
两者都属于固定输入的等分空目标组；仅将AddLearner/PromoteLearner中相同的目标ID
映射为EQUAL_SCORE_EMPTY_TARGET，并验证两处ID一致、位于4..stores。其余步骤、
peer ID、source、leader转移、分数、错误等保持逐字段比较；原始差异完整保存在
raw_target_tie_differences，未抹去操作差异。同一快照的冷/热结果仍严格相同。耗时和分配量仅记录，不参与相等断言。

## 计量与复现

[environment.json](logs/task_155/environment.json)记录机器、OS、rustc、profile和并发。
使用任务要求的test/debug profile，**不是release生产延迟数据**；全局System分配器
包装只统计成功alloc/realloc次数和请求字节，含其原子计数开销，不是RSS、峰值或净
存活内存。RUST_TEST_THREADS=1；单个current-thread Tokio测试内无外部并发任务。
计量区间只包括schedule/solve调用，排除快照/候选构造、缓存预热、断言、JSON与打印。
不清空OS页缓存；cold/warm定义以本测试明确控制的对象与placement缓存为准。

从仓库根执行以下命令可复用动态槽位领取、输出与退出码保存机制：

```sh
python3 docs/rust/schedule-execution/logs/task_155/verify.py repeat \
  'cargo test -p astersql-pd-compat-tests --locked --test schedule_performance -- --nocapture'
```

runner先按1–10原子mkdir动态领取槽位，写plan/task/session/PID owner记录，全部Cargo
及子进程继承绝对CARGO_TARGET_DIR。失败/中断等待子进程退出后释放自己的锁并保留
缓存。另以[runner-failure](logs/task_155/runner-failure.json)主动执行exit 7，确认
runner保留退出码、向调用方返回7并释放自己的锁。实际各阶段使用 **槽位1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
CARGO_BUILD_JOBS=4，聚焦性能target用RUST_TEST_THREADS=1，其它验证用2，默认features。重复结果应在同机器、工具链、profile和负载条件
下比较；没有自定性能阈值。

## 失败与验证证据

首次[编译](logs/task_155/compile.json)记录测试接线/API访问错误；
[initial](logs/task_155/initial.json)、[setup](logs/task_155/setup.json)、
[setup2](logs/task_155/setup2.json)记录遗漏显式policy/max-replicas和不正确kind期望，
随后修正测试装配（搬迁leader时kind实际含Leader），没有修改生产算法。
这些装配错误不是生产行为缺失的证明。

[首次全仓验证](logs/task_155/serial-workspace.json)在既有TSO进程重启测试失败：
RUST_TEST_THREADS=1导致libtest的`test ...`前缀与`PROCESS_TSO`在同一行，而既有
解析器要求行首。该次失败完整保留，没有标为通过；没有修改TSO源码。将非性能验证
恢复为前置任务采用的RUST_TEST_THREADS=2后，[单项复验](logs/task_155/recovery.json)
退出0、1通过。最终runner只对聚焦性能target强制单线程，对其它命令使用双线程，
每条最终记录明确保存实际线程数；随后重新执行全部八条验证。

已有生产算法可用，本任务建立新的测量harness与断言。为了证明同一测试能检出
实际结果错误，临时在harness中丢弃leader操作、把hot实际rank改为5：
[故意变异源码](logs/task_155/mutated-test.rs.gz)与[红测](logs/task_155/red.json)
记录退出101、1通过/2失败，分别断言0≠1、Some(5)≠Some(4)。此为显式故障变异，
没有篡改冻结期望。移除变异后完成下述所有验证。

[verify.py](logs/task_155/verify.py)保存合并stdout/stderr、确切命令、退出码及哈希；
[最终记录](logs/task_155/verified.json)和[数量汇总](logs/task_155/verified-summary.json)：

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务格式变化 |
| `cargo test -p astersql-pd-compat-tests --locked --test schedule_performance -- --nocapture`（两次） | 0 | 每次3通过；36条样本、4条错误/边界案例、666次转移闭环 |
| `cargo test -p astersql-pd-compat-tests --locked` | 0 | 29通过、0失败、1既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1921通过、0失败、1既有忽略 |

[只读Go拦截器](logs/task_155/reuse-go/go)拒绝实际Go构建/测试，仅复用已核验哈希的
既有etcd fixture二进制、只读kvproto ZIP；记录见[go-reuse.jsonl](logs/task_155/go-reuse.jsonl)。
没有make、failpoint注入/启停、Go生成、模块整理或任何Go源文件写入。
[audit.py](logs/task_155/audit.py)对照[开始快照](logs/task_155/baseline.json)检查全部
Go/模块文件集合、内容SHA-256、Git状态（含untracked）、两份plan；验证全部日志
哈希、Markdown链接与git diff --check。最终审计见[audit.json](logs/task_155/audit.json)。
审计共1103个只读文件及plan的内容和Git状态不变；自有槽位锁已释放，缓存保留。
只提交本任务内容，排除已有其它编号任务删除；任务完成后删除155编号文件，不改plan。

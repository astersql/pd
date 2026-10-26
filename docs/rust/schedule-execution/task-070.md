# 任务070：领导者均衡调度

状态：已完成。

前置69提交 `5836197e5` 提供负载比较、真实 pending operator/影响量；
其包级21项及全仓1570项通过证据已审阅，本任务包级回归再次覆盖前置API。
两份 plan.md 保持只读；没有用前置全仓回归缺口阻断实现。

生产路径：[balance_leader.rs](../../../pkg/schedule/schedulers/balance_leader.rs)，
[API与边界](../../../pkg/schedule/schedulers/balance_leader.rs.md)。
导出 BalanceLeaderScheduler、BalanceLeaderConfig、LeaderBalanceContext、
LeaderBalanceResult 和候选排序；复用 BalanceSolver、RetryQuota、真实 core、
filter、placement 与 PeerOperatorFactory。调度入口为异步 schedule；既有同步
Scheduler trait 的宿主接线不在此任务扩展，不安装私有 runtime。

## 来源与固定案例

遵循用户后续授权：**只读 Go 源码/测试推导 + Rust 行为实测**，没有本任务
Go 运行输出或 Go/Rust 运行差分。[provenance](logs/task_070/provenance.json)
保存真实来源 commit、源文件/fixture SHA256；Go binary/output SHA256 为 null。
已审阅 oracle-index，既有执行路线没有本任务行为覆盖，没有借用其运行结论。

来源 `balance_leader.go` 的 Schedule、makeInfluence、transferLeaderOut/In、
createOperator、IsScheduleAllowed、candidateStores 与配置校验。
相邻只读测试 TestBalanceLeaderSchedulerConfigClone 提供深复制契约；
TestBalanceLeaderBatchLimit 提供1..=100边界与非法值保持旧配置；
TestBalanceLeaderSchedulerTestSuite 中 BalanceLimit、SchedulePolicy、TolerantRatio、
ScheduleWithOpInfluence、TransferLeaderOut 对应容忍区间、策略与影响量；
TestBalanceLeaderRangeSchedulerTestSuite 中 MultiRangeBalance、BatchBalance、
ReSortStores 对应范围、去重与候选重排。TestMain 仅作为套件入口，未当作行为证据。
TestBalanceLeaderMaxBatchSchedule 提供最大batch契约，本次在batch=100、8个可用
区域场景验证返回全部8个且不重复，配置上限100另外单测。
core RandomRegions 的采样上限10、空ranges全域语义及1e-6浮点比较也按只读来源核实。

[fixture](fixtures/task_070.json) 冻结31项，期望数来自上述契约的手工推导，
未调用待测实现生成期望：batch 1/4/100、单区去重、平衡/容忍等号/刚超过、
pending influence、无follower、热区、pending、缺失源store、源/目标方向暂停、
范围外/多范围/专用key manager排除、自定义名字、size/count策略、权重、真实placement、
见证/learner目标、无region、批内影响量停止、down peer、specialUse、removing源/目标，
以及follower原始分数差小于1e-6仍选择较低分的边界。
固定随机/时钟规则见provenance，不忽略错误与操作差异。Go follower同分无稳定顺序，
操作目标要求属于允许的同分目标集合；候选队列的ID同分顺序则精确断言。

## Rust实测与故障证据

3项真实测试：

- `task_070_candidates`：复现 Go ReSortStores 的6个store初始序列、binary search、
  source影响后排序、top/bottom重排；分数平局优先较大ID。
- `task_070_config_and_allowed`：batch 0/101类型化拒绝，默认4、clone修改100不影响
  原值，JSON往返及非法batch拒绝；operator count < limit、等于limit、零limit。
- `task_070_behavior_cases`：31/31固定案例；每个成功结果验证真实TransferLeader步骤、
  LEADER kind、唯一region、health check、sourceScore>targetScore；将真实命令应用到
  canonical region及role indexes，更新真实store leader count/size，检查操作Start/Check
  成功及实际分数差缩小。控制器输入pending influence保持原值。
  down/pending诊断为step1 REGION_UNHEALTHY，score拒绝为step3，见证目标的真实工厂
  错误被保留并产生step4 CREATE_OPERATOR_FAILED，不虚构I/O失败。

[red-api](logs/task_070/red-api.log)：先注册候选/config测试时缺失模块/API，退出101。
行为测试装配阶段曾因恒定或相关采样序列只命中已用区域而未填满batch；核实真实
RandomRegions每次取10个后调整显式输入，保留重试与去重契约，没有降低期望。
[red-behavior](logs/task_070/red-behavior.log)：故意反转should_balance拒绝条件，同一
行为测试在batch-four断言0 != 4，2项通过/1项失败，退出101；随后恢复生产代码。
这证明测试可识别错误行为，不只依赖缺包或缺注册失败。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_070 -- --nocapture` | 0 | 3通过，0失败/忽略；31/31固定行为案例 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 24通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage废弃警告 |

确切argv、环境、退出码和输出位于 [logs/task_070](logs/task_070/)。
每次Cargo阶段均由run.py原子mkdir动态领取共享槽位，实际领取 **1、2**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1` 或
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
最终聚焦/包级在槽位2通过，最终全仓在槽位1运行；每条命令JSON记录实际槽位。
每次阶段结束均等待子进程退出后释放自己的锁，保留缓存；正常、失败、中断均清理。

最终 `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked`
退出 **0**，实际 **1573通过、0失败、1既有忽略**，见
[workspace-final.log](logs/task_070/workspace-final.log) 和 [counts.json](logs/task_070/counts.json)。
既有忽略是需要独立Go/Rust进程二进制的tso_processes，没有新增忽略。
首轮workspace通过后，审查修正了follower原始分数排序，并增加小于1e-6差值的第31个
案例；最终focused/package/fmt-check/check和workspace-final对应最终源码。

[audit](logs/task_070/audit.json) 核实1101个Go/模块文件的清单、SHA256和Git状态
完全不变，两份只读plan.md与HEAD一致；其它任务的已有删除不纳入提交。
日志仅规范行尾空白和EOF空行，[log-normalization](logs/task_070/log-normalization.json)
保存规范前后SHA256；业务fixture没有归一化。来源和最终Rust源哈希逐项核验通过。
全部本任务槽位锁已释放，槽位1/2缓存保留。差异及本地证据链接检查通过。

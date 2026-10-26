# 任务105：范围均衡作业调度

状态：已完成。

前置104由提交 `9b2de5f32` 和 [task-104](task-104.md) 核实：真实队列
BalanceRangeJobs/StorageEndpoint/MetaStore，4个聚焦测试、162包测试、1711个workspace
测试证据齐备。本任务串行执行，无子代理；整体、包级和来源主题 plan 均只读。

采用用户后续明确授权，以只读Go源码/测试推导加Rust真实行为和故障验证替代缺失的
Go运行oracle/运行差分。已核实oracle-index没有任务105的runtime cases。
[provenance](logs/task_105/provenance.json) 固定来源commit、原计划基准及源码/fixture
SHA-256，runtime_oracle=null；没有执行Go、failpoints或改写Go/模块文件。

## 生产路径和契约

[balance_range_schedule.rs](../../../pkg/schedule/schedulers/balance_range_schedule.rs)
提供 `BalanceRangeScheduler::{check_job,prepare,schedule,is_balanced,should_balance,report_at}`
及 `RangeReport`、类型化 `BalanceRangeError`。
[模块说明](../../../pkg/schedule/schedulers/balance_range_schedule.rs.md) 说明领域/宿主边界。
lib显式注册模块和独立测试，复用104持久队列、真实core范围索引、placement阈值/fit、
store/region/snapshot过滤、StatisticsSnapshot和PeerOperatorFactory；无新增依赖或feature。
宿主提供范围内OpInfluence、时间、随机输入、ID分配，负责操作提交、HTTP和指标投影。
Go忽略dry-run参数；领域schedule同样执行实际规划，不额外引入dry-run分支。

源码 `balance_range.go` 固定GC/限额/队首开始、超时、prepare、均衡完成的顺序，
取消时删除范围并清零score/expect；评分不叠加影响量，排序叠加影响量，移动准入扣除
双方绝对影响量和一次移动成本。均衡逐store将绝对浮点差截断为整数后累加，允许<=1。
peer角色按follower/leader/learner尝试，目标倒序，existing leader目标走transfer，否则
按旧role move/replace leader。保留RANGE、Medium和四个两位小数评分字段。
超时完成后Go仍prepare：未均衡返回true；若均衡则再次finish产生状态错误，测试固定
此行为。prepare失败保留已开始状态和range登记；没有擅自修改原始语义。

只读测试关联：TestIsBalanced对应小数量截断、开始/取消/清理；TestPrepareBalanceRange
对应原始score及期望份额；TestPlacementRule对应匹配rule的store筛选；TestTIKVEngine
对应target限额、leader transfer/replacement和评分；TestLocationLabel对应拓扑权重及
拒绝降低隔离度；TestTIFLASHEngine对应engine匹配、learner和评分。未声称运行这些Go
测试，Rust使用独立固定场景，不声称完整重放Go测试函数。

## 场景与故障

[fixture](fixtures/task_105.json) 冻结7条shouldBalance输入/期望和79条调度场景。
完整cluster输入由 [task_105_test.rs](../../../pkg/schedule/schedulers/task_105_test.rs)
的run_case与fixture共同定义：默认4个store、6个连续不重叠region，源peer在1/2/3、
leader在1、learner作业的learner在3；目标4无peer。zone为store ID，TiFlash使用
显式engine约束；默认3 voters，learner作业改为2 voters+1 learner。随机index=0、
shuffle恒等、时钟固定Unix纳秒，分配ID1000；均衡案例只有1 region，existing案例
只有3 store，single只有1 store，outside使用[z,zz)，engine_mismatch改作业engine。
拓扑拒绝案例目标4的zone=2，2/3暂停transfer-in，避免回退到合法existing目标。

2个独立有效测试：

- task_105_behavior_cases：7条固定改进判定（含负influence、等号）及逐store浮点
  截断均衡/不均衡；期望直接来自Go表达式。
- task_105_roles_engines_and_faults：79条：3 roles × 2 engines × 6 variants ×
  2 conf-change模式=72，加existing、timeout、outside、engine_mismatch、single、
  invalid及topology_reject。验证限额不开始、开始保存、范围登记、分数、真实操作
  kind/步骤/目标/ID/优先级/影响量、allocation错误、pending过滤、influence抑制、
  balanced完成、超时等号/大于、双finish状态错误、取消清理和报告十秒边界。
  正常案例分别验证sourceScore=6.00、targetScore=0.00；move后的peer总影响量为0、
  目标region_count为+1。existing场景精确验证单步TransferLeader，source=1。

每个非提前结束的普通场景先在真实MemoryStore的MetaStore I/O边界注入begin保存失败，
再注入stall并取消future，两次都保持pending且范围未登记，然后重试成功；超时场景
注入finish保存失败，保留running与range，恢复后完成并清理。无mock次数断言。
仅时钟/随机/ID和I/O故障边界受控。报告用领域快照替代宿主Prometheus注册，归一化
不忽略状态、错误、kind、步骤、target或influence。

## 红灯和验证记录

initial记录缺少API的编译红灯exit101。focused到focused6记录API接线及测试装配错误，
focused7首次48场景通过；lifecycle/edges/topology系列保存扩展验证及修正过程。
[red_behavior.py](logs/task_105/red_behavior.py) 在完整API存在后分别故意把改进计算的
`-1`改成`+1`、把报告间隔`<`改成`<=`；相同测试分别以1项失败/exit101识别。
[red-results](logs/task_105/red-results.json) 和red日志保留证据；脚本finally恢复生产
原始字节。脚本exit0表示两次红灯验证成功，不能解释成错误实现通过。

最终验证见 [final.json](logs/task_105/final.json)，[完整日志](logs/task_105/final.log.gz) 与 [计数](logs/task_105/test-counts.json) 可复核。
所有Cargo通过 [run.py](logs/task_105/run.py) 原子mkdir动态领取共享槽位，实际槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
最终阶段 `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2`，先cargo fmt --all，再聚焦测试、
crate测试、metadata、fmt check、workspace check/test；命令使用默认features及--locked。
正常、失败和中断等待子进程结束后释放自己的锁，保留缓存。


| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅任务生产/测试及lib格式差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_105 -- --nocapture | 0 | 2通过/0失败/0忽略，7+79条fixture |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 164通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过，既有cpu_usage弃用warning |
| cargo test --workspace --locked | 0 | 1713通过/0失败/1既有忽略，含2个文档测试 |

最终串行命令链exit0，所有子命令均exit0；不是零测试准入。
[audit](logs/task_105/audit.json) 核对1119个Go/模块/plan只读文件，内容无变化，
无新增Go/模块文件；文档链接、Python语法和git diff --check均通过。
自己的槽位锁已释放，缓存保留。其它任务既有编号文件删除不纳入提交。

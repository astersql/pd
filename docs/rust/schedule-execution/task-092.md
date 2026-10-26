# 任务092：热点求解和反向交换

状态：已完成。

依赖91已核实：提交 `6a11bf488`，canonical HotPlacementLoads/HotPlacementScope/
same_hot_placement_population API，以及 [task-091](task-091.md) 保存的5项聚焦、
116项包级、1665项workspace通过证据。开始前读取两级只读plan、oracle-index、
ADR-001、依赖基线、移植指南、仓库说明；没有更深层AGENTS.md。

遵照用户后续授权，采用**只读Go源码/测试推导 + Rust真实行为和故障验证**；
没有运行本任务Go测试，没有本任务Go运行输出，不声称Go/Rust运行差分。
[provenance](logs/task_092/provenance.json)记录来源commit、只读Go文件及冻结
[fixture](fixtures/task_092.json)的SHA-256。oracle-index未提供本任务运行结果。

## 实现和边界

生产路径：[hot_solve.rs](../../../pkg/schedule/schedulers/hot_solve.rs)，
[领域说明](../../../pkg/schedule/schedulers/hot_solve.rs.md)，
[独立测试](../../../pkg/schedule/schedulers/task_092_test.rs)。
公开API：HotSolver::{new,solve,compare_src,compare_dst,set_search_revert_regions,
build_operators}、HotSolveCandidate::{new,is_ready_to_build}、HotRevertCandidate、
HotSolveContext、HotSolveResult、HotOperatorDecoration、HotSolveError、hot_step_rank。
仅在lib.rs注册，不改依赖、features、Cargo.lock或Go。

输入是89/90/91已经筛选的同轮candidate和scope，内核枚举正向与反向候选，复用
HotSolution/HotRankV1/HotRankV2的净负载、progressive rank、uniform检查和
better_than。复用已有compare_store的min/max/diff与离散比较，避免复制排名算法。
引用类型编码Go的非nil前提，额外检查peer store/region及reverse成对性。
反向必须满足搜索门控、不同region、peer membership；placement人口不同还须
通过反向source/destination阈值及scope membership。v1不搜索反向；v2每轮结束
按最佳方案更新搜索状态，重复轮次测试覆盖初始关闭到启用。

使用真实PeerOperatorFactory/BuilderContext及StoresInfo/config/RuleManager，
为已存在的target peer转移leader，为不存在的target移动leader或peer并保持role。
每个operator直接设HIGH priority和store health check；返回方向、读写、维度和
counter事件给宿主metrics生命周期。pair全部构建成功才替换best；任何一侧
失败均丢弃新pair、传播typed BuilderError并保留先前可执行赢家。ID可能已消费，
不承诺回滚allocator。此层不提交operator或写pending。

桶拆分（包括build前大region尺寸检查）属于93；读写调度、真实候选准入/snapshot/
affinity/反向fit safeguards的总体装配属于94；pending生命周期属于95。
92不宣称完整balance-hot-region调度器已接线。缺失规则不能绕过builder校验。

## 来源推导及固定样例

Go参考：hot_region_solver.go的solve、compareSrcStore/compareDstStore、stepRank、
isReadyToBuild、buildOperators的非split分支、createOperator和decorateOperator；
rank formulas沿用87/88证据；已有task_031 fixture的move-leader明确包含5步
（add/promote/transfer/remove/transfer），保持canonical builder的中间leader转移。

任务所列TestSplitBucketsBySize/Load/LoadWithCPUFallback验证split分支，归93；
TestHotCacheSortHotPeer/MaxZombieDuration验证peer/cache统计，已由89覆盖，
均不能冒充本任务solve/ready/compare/build/decorate的Go运行证据。
同文件的TestRevertRegionPlacementSafeguard以及population guard测试用于理解
反向准入；实际只读来源函数和邻测语义作为本任务推导依据。

冻结JSON含10条有效记录：4条solve（v1/v2改善、无改善），5条step截断及1条
reverse交换。期望来自源码公式推导，不由待测实现生成。初版reverse样例
main=[400,200]、revert=[200,400]会令正向rank=-1，不满足源码的rank2/0搜索门控；
真实测试失败后按同一源码计算修正为main=[200,400]、revert=[0,600]，净负载
[200,-200]改善两维，期望rank4保持不变。记录保留在focused-dev.log.gz。
不是将Rust实际输出回填期望。测试装配还修正RegionInfo字段访问、借用生命周期
和move-leader既有5步期望；没有放宽生产断言。

7项测试，0忽略：

- task_092_behavior_cases：4条冻结solve，rank/空操作、真实步骤/目标、HIGH/health、维度/方向。
- task_092_step_compare_and_readiness：5条冻结step，peer/region/revert完整性，source/dst/pending比较。
- task_092_reverse_and_atomic_failure：冻结交换rank4双operator，反向方向；第三次分配
  即pair第二侧分配失败时保留此前正向rank2单operator和typed错误。
- task_092_best_tie_uniform_and_stale：v1/v2更好候选获胜、相同候选稳定、uniform跳过、stale/空轮次。
- task_092_real_leader_and_role_builders：真实transfer/move leader/move peer的1/5/3步，
  learner角色保持2步和typed readiness错误。
- task_092_reverse_gates_and_search_rounds：默认不搜索、第二轮搜索、同region/成员不符/
  stale reverse拒绝、v1永不搜索、非法dimensions错误。
- task_092_placement_population_and_builder_fault：真实HotPlacementLoads标签人口[2,4]
  拒绝reverse范围外target；首个allocation故障返回空和typed错误；非法peer limit。

精确ID/rank/错误/步骤/方向不归一化。浮点数学允许1e-9误差（本任务整数fixture
直接精确断言）；保留输入相同候选的先到顺序。JSON中的runtime_oracle显式为null。
所有新增测试使用真实领域对象，仅allocator外部I/O边界注入失败，没有mock计数替代行为。

## 红灯及验证

[red-missing-api](logs/task_092/red-missing-api.log.gz)退出101，为先注册同一测试的
缺API编译红灯。随后[red_behavior.py](logs/task_092/red_behavior.py)临时禁用反向搜索，
同一组真实测试5通过/2断言失败，Cargo退出101；外层确认检测成功退出0。
finally逐字节恢复生产文件，后续fmt和全部回归使用真实实现。

所有Cargo由[run.py](logs/task_092/run.py)每阶段原子mkdir动态领取1–10槽位。
本任务实际各阶段均领取槽位1，绝对CARGO_TARGET_DIR：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
命令、环境和退出码保存于同名JSON；原始stdout/stderr保存于log.gz。
脚本在失败/中断时终止并等待整个子进程组，再清除自己的owner记录/锁；保留缓存。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅本任务Rust文件及lib.rs差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_092 -- --nocapture | 0 | 7通过/0失败/0忽略；10条冻结记录 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 123通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过；既有cpu_usage弃用warning |

`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked`退出0：
1672通过/0失败/1既有忽略（tso_processes），含2项文档测试。
全部槽位产物子进程已结束，slot-1自有锁已释放，缓存保留。
存储回归复用已有只读Go fixture源码构建二进制，不代表本任务Go运行oracle。
最终日志计数由
[counts.py](logs/task_092/counts.py)解析，零测试组不作为行为验收。

[focused](logs/task_092/focused.log.gz)、[package](logs/task_092/package.log.gz)、
[workspace-test](logs/task_092/workspace-test.log.gz)、
[red-behavior](logs/task_092/red-behavior.log.gz)。

只读审计：开始快照1119个Go/模块/plan文件，结束时逐SHA-256核对并扫描untracked；
执行[audit.py](logs/task_092/audit.py)，证据见[audit.json](logs/task_092/audit.json)。
审计退出0：1119个哈希完全一致，Go状态为空，git diff --check与原主题diff均退出0。
开始时78/80–91任务文件删除是既有变更，不纳入本任务提交。两级plan和原主题保持只读。

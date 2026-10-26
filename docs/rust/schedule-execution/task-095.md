# 任务095：热点待定影响生命周期

状态：已完成。

依赖94已核实：提交 `4b59aae58`，`HotScheduler::dispatch` 提供有序真实
`HotSolveResult` offers；[task-094](task-094.md)记录5项聚焦、133项包级与1682项
workspace测试通过。两级plan只读，原主题未修改，未发现更深AGENTS。

按用户后续明确授权采用**只读Go源码/测试推导 + Rust真实行为和故障验证**，
无本任务Go运行oracle，未声称Go/Rust运行差分。oracle-index没有本任务必要输出。
[provenance](logs/task_095/provenance.json)保存来源commit、Go源码/测试、fixture与
oracle-index的SHA-256；[fixture](fixtures/task_095.json)显式source_assertion、
runtime_oracle=null。仅使用显式monotonic clock，无负载、目标、状态或错误归一化。

## 实现

[hot_pending.rs](../../../pkg/schedule/schedulers/hot_pending.rs)扩展
`HotLoadProjection::{try_add_pending_influence,admit_hot_solution,admit_hot_split}`，
提供 `collect_hot_pending_influence`、`HotDispatchResult::admit_pending` 和
`HotPendingError`；[领域说明](../../../pkg/schedule/schedulers/hot_pending.rs.md)。
局部lib注册测试；修正hot_schedule注释，避免把单region准入误称整对原子操作。
Cargo.toml/Cargo.lock未改动。

复用任务69/86已有的真实operator状态、pending权重与GC；复用89的zombie来源选择，
94的真实读写方案、92的实际operator builder。成功保持影响至结束后的zombie到期，
取消/超时/过期/被替换立即为零但保留记录到窗口结束。重复region准入不覆盖任何字段；
每轮使用新store summary，重复心跳只计算一轮影响。八维读写负载从main及逆向cache
收集，缺少逆向cache时该方向置零，热peer数为1。陈旧/短快照返回typed error。

引擎完全匹配（TiKV、TiFlash Write、Compute）；split从所有副本扣除影响，目的0。
读首选准入失败可尝试另一真实offer。Go源码明确TODO整对原子性：主记录登记后反向
冲突，主记录保留；本次忠实保留并以断言验证。完整宿主snapshot/cache/提交/指标装配
仍由后续facade及集群任务验收。本任务无持久化外部写入，不新增MetaStore/mock。

## 来源与覆盖

只读审查 hot_region.go tryAddPendingInfluence、summaryPendingInfluence、
balanceHotReadRegions 和 hot_region_solver.go tryAddPendingInfluence、
collectPendingInfluence、calcMaxZombieDur；源码推导预期在实现前冻结。

指定Go对照测试已按真实内容审查：TestUpgrade、TestReloadConfigKeepsPendingWeightDefault、
TestCreateSchedulerRejectsPendingWeightBelowOne属于85的配置迁移/默认值/校验，
不算本任务运行覆盖；TestGCPendingOpInfos覆盖两种placement模式的取消/未结束/到期
记录GC；TestMaxZombieDuration包含6种来源窗口。本次Rust真实测试覆盖相应状态和
窗口，不声称执行这两个Go测试。TestSplitIfRegionTooHot包含重复调度受pending抑制；
本次覆盖split真实操作器准入去重/全副本影响，93已有拆桶决策；
TestSplitBucketsBySize、TestSplitBucketsByLoad、TestSplitBucketsByLoadWithCPUFallback
主要覆盖93的桶split选择；TestHotCacheSortHotPeer是89的peer排序，不虚报为pending
生命周期案例。没有运行需要failpoint的Go测试或任何Go写入命令。

5项独立真实测试，显式注册、无忽略：

- `task_095_behavior_cases`：11条冻结生命周期输入×2轮summary=22轮断言，
  pending weight=2.5，成功边界29/30秒，取消/超时/过期/替换，真实自动故障状态。
- `task_095_collect_and_duplicate_faults`：4条冻结八维采集案例（读/写，有/无inverse）；
  每条验证重复不覆盖、两类陈旧inverse身份、短负载typed error。
- `task_095_dispatch_admission_fallback_and_reverse_conflict`：v1/v2两种公式的真实
  solver→dispatch→pending→leader备选；真实主/反向operator builder成功及反向冲突。
- `task_095_split_and_admission_errors`：真实SPLIT、三副本来源、无目的影响，
  重复store快照去重、重复split、陈旧region、缺少方案、跨引擎错误且失败不插入。
- `task_095_zombie_provenance_and_typed_errors`：6种来源窗口30/180秒、
  每种均验证非法配置和陈旧operator/region typed error。

红灯 [red-api](logs/task_095/red-api.log.gz)：缺少API编译失败，Cargo退出101。
[red_behavior.py](logs/task_095/red_behavior.py)临时将登记origin置零：同一
behavior_cases得到0 != -25，0通过/1失败，Cargo退出101；外层确认预期失败退出0，
finally逐字节恢复。恢复后全部聚焦测试通过。split测试初次错误地给factory传入
零kind，导致StaleRegion，已改为真实调用要求的SPLIT kind；保留原始失败日志，
没有放宽生产准入条件或忽略测试。
红灯 [red-split-dedup](logs/task_095/red-split-dedup.log.gz)：重复store快照得到
[1,1,2] != [1,2]，0通过/1失败，退出101；改用真实 `RegionInfo::store_ids` 集合
后同一断言通过。此修正后重跑聚焦、包级和workspace全部验证。

## 验证

每阶段使用[run.py](logs/task_095/run.py)从1–10原子mkdir动态领取共享槽位，
记录任务/会话/PID；失败或中断先终止并等待子进程组再清自己的锁，缓存保留。
所有阶段实际领取槽位 **1**，绝对 CARGO_TARGET_DIR：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
确切命令、exit code及环境在各同名JSON，stdout/stderr完整保存在log.gz。

所有阶段与使用槽位产物的子进程均已结束，自有锁已释放，编译缓存保留。

| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅本任务Rust格式差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_095 -- --nocapture | 0 | 5通过/0失败/0忽略 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 138通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过，仅既有cpu_usage弃用warning |
| CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked | 0 | 1687通过/0失败/1既有忽略，含2项文档测试 |

[计数](logs/task_095/counts.json)由[counts.py](logs/task_095/counts.py)从完整日志解析；
[focused-final](logs/task_095/focused-final.log.gz)、[package](logs/task_095/package.log.gz)、
[workspace-test](logs/task_095/workspace-test.log.gz)保存最终原始输出。
`*-before-dedup`日志是增加重复store边界之前的通过记录，不代替最终重跑证据。

[audit.json](logs/task_095/audit.json)与[audit.py](logs/task_095/audit.py)核对开始/结束
1119个Go/模块/plan文件SHA-256一致，含untracked扫描；原始哈希集保存在
[protected-hashes](logs/task_095/protected-hashes.json.gz)。Go Git状态为空，
git diff --check退出0，无Go/模块/Cargo锁差异。两级plan及来源主题未改动。
开始时78、80–94任务文件删除属于既有变更，不纳入本提交。

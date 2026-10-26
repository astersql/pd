# 任务090：热点目标节点筛选

状态：已完成。

前置89已核验：提交 `8d2c8b602`、[task-089](task-089.md) 保存7个聚焦、104个包级、
1653个workspace测试通过证据；canonical HotSourceSelector、HotSourceOperation、
StoreLoadDetail、HotPeerStat、RegionInfo、RuleManager、PlacementGuardContext、
HotRegionParams 均存在。已读取整体/包级只读计划、oracle-index、ADR-001、
依赖基线、移植指南及仓库说明。

遵照用户后续明确授权，以**只读 Go 源码/测试推导 + Rust真实行为和故障验证**替代
缺失的Go运行oracle/运行差分。**没有运行本任务Go测试，没有目标筛选Go运行输出，
不声称取得Go/Rust运行差分**。oracle-index未更新，原始主题和两份plan.md保持只读。

## 实现与来源

生产路径：[hot_destination.rs](../../../pkg/schedule/schedulers/hot_destination.rs)，
[领域说明](../../../pkg/schedule/schedulers/hot_destination.rs.md)，
[独立测试](../../../pkg/schedule/schedulers/task_090_test.rs)。
公开API：HotDestinationSelector::{new, destination_store_decision, pick_dst_stores,
filter_dst_stores, allow_revert_region, is_revert_region_valid}；HotDestinationScope、
HotDestinationDecision::{label,admitted}、HotDestinationError。仅增加lib.rs模块/测试
注册，不改Cargo依赖、锁文件或features。

只读来源 `hot_region_solver.go:714–944` 的目标候选、目标阈值、历史、scope与reverse
规则保护；`:231–240` 的reverse membership；rank_v1/v2的first/all/any优先级分派。
[provenance](logs/task_090/provenance.json)保存来源提交 `8d2c8b602`、Go源码/测试及
[冻结fixture](fixtures/task_090.json) SHA-256。10组fixture在实现前冻结，输入/输出
均标记source_assertion。期望来自Go分支与断言，未从待测Rust实现生成。
目标结果只比较store ID集合（Go返回map）；负载、阈值、错误和状态不归一化。

已只读核实任务列出的Go测试：TestSplitBucketsBySize、TestSplitBucketsByLoad、
TestSplitBucketsByLoadWithCPUFallback验证split operator及分裂键，不覆盖本任务；
TestHotCacheSortHotPeer验证peer优先级上限，TestMaxZombieDuration验证统计来源窗口，
属于已完成89的行为，不能当作目标筛选运行证据。
实际额外参考TestDestinationStoreFailureRejectsStoreOutsidePlacementScope的先判人口
后访问history，以及TestRevertRegionPlacementSafeguard的唯一标签副本迁回拒绝断言。
以上均为源码推导。

## 真实行为和故障覆盖

7个独立测试，至少40个表驱动检查点全部通过，另有显式边界和故障断言：

| 测试 | 覆盖与计数 |
| --- | --- |
| task_090_behavior_cases | 10组冻结输入；严格边界、future最大值、v1 first/all、v2 any、write leader first、TiFlash修正 |
| task_090_history_scope_and_faults | 4组history模式、3组无效维度；history严格边界、scope先判成员/覆盖expect、CPU、malformed history捕获panic |
| task_090_candidate_actions_and_empty | 7组read/write×move/leader×leader资格；既有peer排除、learner/voter follower差异、空统计和无冷目标 |
| task_090_engines_and_real_store_filters | 4组引擎/开关组合、compute拒绝、reserved过滤、真实heartbeat超时 |
| task_090_revert_real_rules | 2组reverse leader模式、4组reverse membership；另2组reverse peer、2组forward peer规则；唯一b副本替换拒绝 |
| task_090_eviction_scope_and_recovery | 3组真实eviction/recovery共6个结果；另3组scope结果、existing follower与new-store move检查区别、leader pause/recovery、允许hotRegion用途 |
| task_090_location_and_witness_safeguards | 无规则时位置隔离拒绝/保持；真实规则fit下witness拒绝、普通leader允许、forward leader筛选 |

40=10+4+3+7+4+2+4+6，计数不把额外显式断言重复混入表驱动组。
全部使用真实StoreLoadPred、StoreInfo、StoreStateFilter、HotRegionEvictedTargetFilter、
SpecialUseFilter、placement guard和RuleManager。MemoryStore支持真实规则初始化/
持久化；此筛选层没有新增I/O，不制造无关持久化故障点。仅控制显式输入与时钟，
不以mock调用次数代替状态和目标结果。畸形history保持Go程序前置条件panic语义。

范围边界：本任务不构造operator，不复制placement人口索引或全局metrics；population
构造、reverse同region/health/stale检查及reverse scope负载由后续solver装配。
生产说明明确当前context/fit/source/peer同轮且store统计ID唯一的前置条件。

## 失败验证与最终验证

[red-missing-api](logs/task_090/red-missing-api.log.gz)先注册同一测试，缺失模块/API
退出101，仅算编译红灯。
[red_behavior.py](logs/task_090/red_behavior.py)在取得初次通过后将预测最大值故意改成
最小值；同一过滤命令4通过/1失败、退出101，future-hot实际dst-store-succ、期望
为dst-store-failed。finally逐字节恢复生产文件，后续补边界测试并运行最终验证。
保留green-initial记录中的fixture错误：空engine标签触发真实exclusive-label保护，
修正为Go空label map语义后通过。green/package-pending-disabled记录显示原始
ProviderSnapshot的pending上限为0会禁用限制；测试明确设置64后验证真实限制。
这些是测试输入修正，没有放宽生产谓词或删除失败断言。

所有Cargo经[run.py](logs/task_090/run.py)从1–10动态原子mkdir领取共享槽位，记录
计划/任务/会话/PID，子进程继承绝对CARGO_TARGET_DIR，正常、失败、中断在子进程
结束后释放自己的锁，保留缓存。各命令环境、退出码及槽位保存在同名JSON。
全部使用槽位1，绝对 `CARGO_TARGET_DIR` 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅任务相关Rust文件差异 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_090 -- --nocapture` | 0 | 7通过/0失败/0忽略 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 111通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过；既有cpu_usage弃用警告 |

`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` 退出0，
1660通过/0失败/1既有忽略，含2个文档测试；既有忽略项为tso_processes。
全部测试和槽位产物子进程已退出，自有slot-1锁已释放，编译缓存保留。
workspace存储测试复用既有只读Go etcd fixture源码编译二进制，运行产物位于
领取槽位；这是存储基线，不是本任务目标筛选Go运行oracle。

原始stdout/stderr以 `.log.gz` 压缩保留，确切命令、环境、槽位和退出码为 `.json`。
[聚焦](logs/task_090/green.log.gz)、[包级](logs/task_090/package.log.gz)、
[workspace](logs/task_090/workspace-test.log.gz)、[红灯](logs/task_090/red-behavior.log.gz)。
[counts.py](logs/task_090/counts.py)从实际日志汇总[counts.json](logs/task_090/counts.json)。
零测试组不作行为覆盖证明。

`python3 docs/rust/schedule-execution/logs/task_090/audit.py`比对开始快照中的1119个
Go/模块/只读plan文件SHA-256，含untracked扫描；[audit](logs/task_090/audit.json)
记录无差异、Go状态为空、diff check与原始主题差异检查退出0。
初始78/80–89任务删除不纳入本任务提交。

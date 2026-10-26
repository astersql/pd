# 任务093：热点桶拆分决策

状态：已完成。

依赖92已核实：提交 `c337bbd33`，canonical HotSolver、HotSolveCandidate、
HotSourceOperation 以及 [task-092](task-092.md) 的7项聚焦、123项包级、
1672项workspace通过证据。已读两级只读plan、ADR-001、依赖基线、移植指南、
仓库规则；无更深AGENTS.md。oracle-index没有093运行输出。

按用户后续明确授权，本任务采用**只读Go源码/测试推导 + Rust真实行为和故障
验证**，没有运行Go测试，不声称Go/Rust运行差分。
[provenance](logs/task_093/provenance.json)保存来源提交与六份只读Go源码/测试、
[冻结fixture](fixtures/task_093.json) SHA-256，runtime_oracle为null，不归一化结果。

## 实现与范围

生产路径：[hot_split.rs](../../../pkg/schedule/schedulers/hot_split.rs)，
[领域说明](../../../pkg/schedule/schedulers/hot_split.rs.md)，
[独立测试](../../../pkg/schedule/schedulers/task_093_test.rs)。
API：HotSplit::{new,bucket_first_stat,too_hot_need_split,by_load,by_size,
for_hot_region,before_move}、bucket_first_stat、is_bucket_load_dim_supported、
HotSplitResult、HotSplitError。

复用canonical core桶、statistics-buckets桶统计和真实operator builder；阈值门控
直接调用HotRankV1/V2::check_loads，复用严格挑选/写leader/any语义。负载半分点
保持uint64 wrapping、端点排除、end越界回退start，以及accLoads/totalLoads。
按大小对有效内部key取上中位数。真实operator保持USEKEY、SPLIT、range、epoch
和读写描述，rank5表示方案优先级，不把operator优先级改成HIGH。

for_hot_region无key时允许继续均衡；before_move仅peer移动且size严格超限时
返回Some（含空operator情况），阻止过大region无key时回退移动；主/反向region
按输入顺序处理。每region builder故障跳过该region并在errors中保留类型及ID，
其它成功split保留；成功才rank5。直接by_load/by_size仍传播typed error。
完整scheduler/cache准入与调度装配属于94，pending属于95，本任务不扩展它们。

仅增加lib注册以及两个已存在workspace依赖（statistics-buckets、hbstream）；
Cargo.lock仅新增这两个路径依赖。离线generate-lockfile一度升级pulldown-cmark，
已恢复既有0.13.4/checksum；远程依赖无变化。不写Go、模块、plan或原主题。

## 推导与案例

参考hot_region_solver.go的bucketFirstStat、splitBucketsOperator、
splitBucketsByLoad、splitBucketBySize、createSplitOperator、buildOperators，
hot_region.go的tooHotNeedSplit及rank_v1/v2的checkByPriorityAndTolerance。

- TestSplitBucketsBySize的3条边界、TestSplitBucketsByLoad的3条区间及
  TestSplitBucketsByLoadWithCPUFallback的CPU回退对应冻结案例。
- 额外TestBucketFirstStat的6条read/write/key/query/CPU回退采用原测试预期。
- TestSplitIfRegionTooHot用于阈值触发及提前返回的控制流推导；本任务不声称完整
  Go scheduler端到端运行。Rust阈值等值、v1 strict/non-strict、v2 any、写leader
  first-only和真实bucket报告明确验证领域分支。
- TestHotCacheSortHotPeer、TestMaxZombieDuration属于89的cache/peer选择；
  TestUpgrade、TestReloadConfigKeepsPendingWeightDefault、
  TestCreateSchedulerRejectsPendingWeightBelowOne属于85配置；TestGCPendingOpInfos
  属于pending生命周期。它们不提供本任务运行oracle，也没有只列名字计作通过。

冻结fixture含17条记录：4条size、7条load、6条dimension。
行为测试实际执行38条表驱动案例（4 size + 7 load × 2方向 × 2优先级 + 6维度），
另有6条阈值表和真实BucketReport转换后6条维度/方向表。
故障及边界还覆盖空/单/零负载/区间不匹配、uint64回绕、binary/unbounded key、
非法维度/方向、CPU fallback、反向大region、禁用bucket、leader不按size拆分、
超限无key、joint-state builder错误及批量前后顺序下成功split保留。
测试没有mock调用次数或外部持久化写入，纯领域无需MetaStore。

## 红灯和验证

初次--locked因新增本地依赖而退出101，记录red-missing-api；随后离线生成lock，
[red-api](logs/task_093/red-api.log.gz)同一测试因缺HotSplit API编译失败，退出101。
初次实现后的focused-dev为2通过/1失败，原因是测试替换报告时漏传旧Arc，CAS
保留前一条报告；改为每条size用新RegionInfo并断言更新成功，没有修改fixture预期。

[red_behavior.py](logs/task_093/red_behavior.py)将上中位数临时改成首key，同一
behavior_cases精确断言b != d失败；Cargo退出101（最终5项测试为3通过/2失败），外层检测
成功退出0，finally逐字节恢复生产源码。此后验证使用恢复后的真实实现。

每个Cargo阶段都由[run.py](logs/task_093/run.py)从1–10原子mkdir动态领取槽位，
记录PID/任务/会话；退出与中断终止并等待子进程组，清理自己的记录与锁，保留缓存。
各阶段命令、退出码和环境保存在同名JSON，原始输出为log.gz。

5项真实测试均显式注册，0忽略：

- task_093_behavior_cases：38条冻结表驱动案例，精确key、kind、range、policy、
  direction描述、accLoads/totalLoads。
- task_093_threshold_and_gates：6条阈值表、rank5、禁用、等于上限、oversized无key。
- task_093_errors_and_boundaries：非法输入、空/单/无效桶、joint builder、uint64回绕。
- task_093_real_bucket_report_and_reverse_size：真实BucketReport转换、6条read/write
  byte/key/query与CPU回退、reverse大小拆分、leader跳过、binary/unbounded。
- task_093_batch_builder_failure_keeps_other_splits：两个输入顺序中joint region失败
  仍保留另一个split；全部失败时rank0和typed error。

最终测试未改动冻结预期；没有以零测试组替代行为验收。

实际各阶段均动态领取槽位 **1**，绝对CARGO_TARGET_DIR为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
全部阶段已结束，自有slot-1锁已释放，编译缓存保留。

| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅本任务Rust文件及lib.rs差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_093 -- --nocapture | 0 | 5通过/0失败/0忽略；38条冻结行为表 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 128通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过；既有cpu_usage弃用warning |
| CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked | 0 | 1677通过/0失败/1既有忽略；含2项文档测试 |

[最终计数](logs/task_093/counts.json)由[counts.py](logs/task_093/counts.py)从完整日志
解析；[focused](logs/task_093/focused.log.gz)、[package](logs/task_093/package.log.gz)、
[workspace-test](logs/task_093/workspace-test.log.gz)、
[red-behavior](logs/task_093/red-behavior.log.gz)。workspace-before-batch为补充批量故障
测试前的额外通过记录，最终验收使用workspace-test。存储回归复用既有只读Go
fixture源码构建二进制，不构成本任务的Go运行oracle。

审计[audit.json](logs/task_093/audit.json)：1119个Go/模块/plan文件逐一SHA-256一致，
Go Git状态为空，包含untracked扫描，git diff --check及原主题diff退出0。
两级plan、原主题保持只读。开始时78/80–92的任务文件删除为既有变更，不纳入提交。

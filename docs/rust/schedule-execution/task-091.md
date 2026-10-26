# 任务091：热点放置范围负载

状态：已完成。

前置90已核实：提交 `3c4d258ce` 与 [task-090](task-090.md) 的7个聚焦、111个
包级、1660个workspace测试通过记录，以及canonical HotDestinationSelector/
HotDestinationScope/HotDestinationDecision API。已读取两级只读计划、oracle-index、
ADR-001、移植指南、依赖基线及仓库说明。

按用户后续授权，本任务采用**只读Go源码/测试推导 + Rust真实行为及故障验证**。
没有运行本任务Go测试，没有本任务Go运行输出，不声称Go/Rust运行差分。
[source provenance](logs/task_091/provenance.json)记录来源commit、Go文件SHA-256与
[fixture](fixtures/task_091.json)哈希。初次5组fixture在生产实现前保存，期望来自
`TestPlacementLoadScopePreservesExpectationGuards`，并非从待测实现生成。

## 实现与边界

生产路径：[hot_placement.rs](../../../pkg/schedule/schedulers/hot_placement.rs)，
[领域说明](../../../pkg/schedule/schedulers/hot_placement.rs.md)，
[独立测试](../../../pkg/schedule/schedulers/task_091_test.rs)。
API：PlacementScopeKey::new、HotPlacementLoads::{new,scope,prepare_for_region,
cached_scopes}、HotPlacementScope::{store_ids,contains,population_words,
destination_scope}、same_hot_placement_population、HotPlacementError。
只增加lib.rs注册；不改依赖、features、Cargo.lock或Go代码。

Go只读来源：`hot_region_solver.go:954–1176`；实际参考scope expectation、value-key
cache、same population、stale index、stale labels、witness exclusion与out-of-scope
测试。任务列出的TestSplitBucketsBySize/Load/LoadWithCPUFallback属于split operator，
TestHotCacheSortHotPeer/MaxZombieDuration属于peer/cache统计，不能充当本任务运行证据。

只在同引擎匹配人口非空且比全引擎更窄时生成局部expect/history/hot-peer count/stddev。
统计使用canonical StoreLoadSummary的当前负载；不使用future或全局expect反推局部值。
规则按constraint值与engine编码，忽略Rule指针/ID/range/version；范围规则由真实
RuleManager对region选出。匹配完整人口或无人匹配均缓存None。leader union只包含
同角色非witness fits，peer move只取source rule；缺失fit/peer/rule、witness或source
标签与fit不一致回退全局。接入90的destination scope，在history访问前诊断范围外。

快照为每轮owned且不可变，索引直接从此快照构造，避免Go中两个独立快照的stale-index
问题；store/label/load变化构造新round并重新建cache。重复ID返回typed error。
位图用于本轮编码；跨轮相同人口用排序ID比较，避免索引位置别名。调用者负责同轮
region/fit/source及rank/config门控；此层不构造operator或安装全局metrics，没有新增I/O。

## 真实行为与故障

5个独立测试，0忽略。冻结表驱动5组（TiKV target/other、TiFlash target/other、空匹配），
另有以下明确边界检查，不把assert数量冒充案例数量：

- task_091_behavior_cases：人口ID、均值、history、hot peer count、归一化stddev和membership。
- task_091_cache_and_snapshot_faults：等值重分配命中同Arc；同人口不同约束；不同人口/engine；
  nil相等规则；空/完整人口与cached miss；规则变化；新快照负载刷新且旧scope不变；
  duplicate-ID typed error；空快照；130-store三字位图；Unicode/分隔符字节长度key。
- task_091_scoped_destination_diagnostic：真实90 selector拒绝范围外store，畸形history不被访问。
- task_091_real_range_fit_and_stale_labels：真实MemoryStore持久化规则、RuleManager按m边界
  分别选择target/other规则；局部均值10/100；缺fit/peer、stale source labels回退。
- task_091_leader_union_excludes_witness：真实fit_region；move source scope、leader跨同角色
  union回退全局、witness source回退、witness-only其它pool不进入leader人口。

正常、边界及故障都验证真实状态/统计/诊断结果。仅显式输入可控，没有mock调用计数。

## 红灯及最终验证

[red-missing-api](logs/task_091/red-missing-api.log.gz)退出101，先注册同一测试，缺失API；
仅算编译红灯。最初实现验证在TiFlash上返回None，原因是fixture规则未显式约束exclusive
engine标签；按Go相邻测试补入engine NotIn other约束，保持原期望及生产匹配语义。
真实RuleManager测试装配期修正了既有API签名，未放宽生产断言。

[red_behavior.py](logs/task_091/red_behavior.py)故意移除人口约束过滤，5个同一测试全部因
预期存在的窄scope缺失而失败，Cargo退出101；外层脚本确认检测成功后退出0。
finally逐字节恢复生产文件，随后执行最终fmt及全部回归。

所有Cargo由[run.py](logs/task_091/run.py)原子mkdir动态领取1–10共享槽位；本任务各阶段
实际均领取槽位1，绝对CARGO_TARGET_DIR：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
命令、环境与退出码见同名JSON；stdout/stderr以log.gz保留。子进程结束后释放自有锁，保留缓存。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅本任务Rust差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_091 -- --nocapture | 0 | 5通过/0失败/0忽略；5组冻结案例 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 116通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| CARGO_BUILD_JOBS=1 cargo check --workspace --locked | 0 | 通过；既有cpu_usage弃用警告 |

ID结果按升序比较，Go map/位图内部位置顺序不作为行为输出；key按UTF-8字节长度编码。
浮点均值/history保持原值；stddev允许1e-9数值误差，不忽略人口、错误或诊断差异。

`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked`退出0：
1665通过/0失败/1既有忽略，含2个文档测试；既有忽略为tso_processes。
全部Cargo及槽位产物进程已结束，slot-1自有锁已释放，缓存保留。
存储基线复用既有只读Go etcd fixture源码编译二进制，仅用于workspace回归，
不代表本任务热点placement的Go运行oracle。

[聚焦日志](logs/task_091/focused.log.gz)、[包级日志](logs/task_091/package.log.gz)、
[workspace日志](logs/task_091/workspace-test.log.gz)、[行为红灯](logs/task_091/red-behavior.log.gz)。
[counts.py](logs/task_091/counts.py)解析实际日志并生成[counts.json](logs/task_091/counts.json)；
零测试组不作为行为验收证据。

`python3 docs/rust/schedule-execution/logs/task_091/audit.py`退出0：
1119个Go/模块/只读plan文件SHA-256与开始快照完全一致，含untracked扫描；
Go状态为空，`git diff --check`及原始主题差异检查退出0。
[audit记录](logs/task_091/audit.json)。两级plan.md与原始主题均未改动。
开始时78/80–90任务删除为既有状态，不纳入本任务提交。

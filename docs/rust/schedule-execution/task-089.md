# 任务089：热点源节点与区域筛选

状态：已完成。

前置88核验：提交 `d26211d1c`、[task-088](task-088.md) 中5个聚焦测试、97个包级
及1646个workspace测试的通过记录；canonical HotRankV2、StoreLoadDetail、
HotPeerStat、RegionReplicationContext、RuleManager 及 HotRegionParams 均存在。
读取整体/包级只读计划、oracle-index、ADR-001、依赖基线、移植指南和仓库说明。
用户后续明确授权以**只读 Go 源码/测试推导 + Rust 真实行为及故障验证**替代
缺失的 Go 运行 oracle/运行差分。**本任务没有运行 Go 测试，没有源筛选 Go
运行输出或运行差分**。oracle-index 原有未豁免要求不冒充已取得的运行证据。

## 实现与来源

生产路径：[hot_source.rs](../../../pkg/schedule/schedulers/hot_source.rs)，
[领域说明](../../../pkg/schedule/schedulers/hot_source.rs.md)，
[测试](../../../pkg/schedule/schedulers/task_089_test.rs)。
API：HotSourceSelector::{new,filter_src_stores,source_store_decision,expect_load,
filter_hot_peers,sort_hot_peers,get_region,max_zombie_duration}，HotSourceDecision
及其 admitted/label、HotSourceOperation、HotSourcePeers 和 HotSourceError。
局部注册lib模块/独立测试，将既有statistics-utils从dev依赖提升至生产依赖，
没有新增远程依赖、feature或Cargo.lock变更。

[provenance](logs/task_089/provenance.json)保存来源提交、5个只读 Go 源码/测试
及[冻结fixture](fixtures/task_089.json) SHA-256。实现前冻结13组源节点和4组peer
上限输入/期望，evidence_kind=source_assertion，go_runtime_executed=false。
Peer union 只比较无序成员，因为 Go 遍历map；equal-rate排序无稳定次序保证。
不归一化阈值、错误、状态或目标。期望来自Go分支/测试断言，没有从待测Rust生成。

已只读核实任务列出的5条Go测试：
- TestSplitBucketsBySize：bucket启用、大小与边界键控制split operator，3组键和禁用场景。
- TestSplitBucketsByLoad：3组bucket负载/边界键生成split operator。
- TestSplitBucketsByLoadWithCPUFallback：CPU无bucket统计，fallback按byte选择d键。
  这三条是split构造行为，**不能作为本任务源节点/peer筛选证据**，留给后续operator故事。
- TestHotCacheSortHotPeer：query/byte优先cap=1、2及CPU/byte cap=1；本任务复用其输入断言。
- TestMaxZombieDuration：read peer/leader、TiKV write peer、TiFlash write peer、
  write leader key/query，共6种统计来源窗口，本任务逐一验证。

此外读取HotReadRegionScheduleWithSmallHotRegion的cooldown/pending及topn场景，
证明nth计算位于剔除之前；具体源码为hot_region_rank_v2_test.go。
阈值和history逻辑追溯hot_region_solver.go及rank_v1/v2的first/all/any分派。
这些都是源码阅读，未将测试名字当作Go运行证据。

## 真实行为与故障

7个独立Rust测试使用真实load prediction、HotPeerStat、RegionInfo与RuleManager。
- task_089_behavior_cases：13组冻结源节点输入，包括严格阈值、future最低值、
  v1 first/strict all、v2 any、write leader、TiFlash修正/读拒绝/compute、scope延期/全局。
- task_089_peer_sort_and_pending：4组冻结cap（0/1/2/3），nth双维引用及pending后不补位。
- task_089_history_scope_and_errors：history严格边界、any维通过、scope期望覆盖、
  malformed历史panic捕获、3组无效维度、负cap、无peer、TiFlash开关。
- task_089_cooldown_topn_and_pointer_union：冷却到期前/恰好到期/未来时间，
  nth不足、pointer去重与相同ID不同对象、列表耗尽/零cap、CPU第一优先级。
- task_089_real_region_placement_and_stale：2模式×10组=20组，真实MemoryStore
  支撑RuleManager初始化。包含missing、healthy、pending允许、down拒绝、
  副本不足/过多/learner、stale peer、stale leader和有效leader；核验成功fit。
- task_089_zombie_provenance：6组窗口选择输入输出。
- task_089_priority_history_modes_and_engine_scopes：4组history优先级模式、
  两引擎独立scope延期、admitted状态、CPU负载与NaN拒绝。

已计数的表驱动组至少47组（fixture17 + region20 + zombie6 + history模式4），
全部成功；另外的边界、错误、故障断言逐项见测试，不混入冻结fixture计数。
此层没有新增持久化或网络I/O；仅控制显式时钟和输入，没有mock调用计数。
完整placement人口发现、目标筛选和operator构造由后续solver负责。

## 红灯与验证

red-missing-api：先注册相同fixture行为测试，缺失模块/API编译错误，退出101，
不计作行为红灯。green-initial另保存一次测试借用临时数组的编译错误，已修正。
[red_behavior.py](logs/task_089/red_behavior.py)故意将current/future的min改为max，
同一过滤命令5通过/1失败，退出101，future-low实际src-store-succ、期望
src-store-failed。finally逐字节恢复生产文件，随后增加边界测试并通过最终验证。

所有Cargo通过[run.py](logs/task_089/run.py)动态原子mkdir领取共享槽位1–10，
记录计划/任务/会话/PID，继承绝对CARGO_TARGET_DIR，失败/中断等待子进程退出
后释放自己的锁并保留缓存。确切命令/环境/退出码保存在同名JSON。

全部使用**槽位1**，绝对 `CARGO_TARGET_DIR` 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

所有测试与子进程已退出，自有slot-1锁已释放，缓存保留。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务Rust文件差异 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_089 -- --nocapture` | 0 | 7通过/0失败/0忽略；47组表驱动及其它边界/故障 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 104通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过；既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1653通过/0失败/1既有忽略，含2个文档测试 |

[有效计数](logs/task_089/counts.json)由[counts.py](logs/task_089/counts.py)从实际
日志汇总，零测试组不作行为证据。既有忽略项为tso_processes。
各命令的原始stdout/stderr以同名 `.log.gz` 压缩保留，环境/槽位/退出码为 `.json`。
[聚焦](logs/task_089/green.log.gz)、[包级](logs/task_089/package.log.gz)、
[workspace](logs/task_089/workspace-test.log.gz)、[红灯](logs/task_089/red-behavior.log.gz)。

`python3 docs/rust/schedule-execution/logs/task_089/audit.py`、`git diff --check`
及文档相对链接检查均退出0。[audit](logs/task_089/audit.json)与开始快照逐一比较
1119个Go/模块/只读plan文件SHA-256，含untracked扫描，无差异且Go状态为空。
原始主题保持不变。workspace存储基线复用已有只读Go etcd fixture源码编译二进制，
产物与WAL在领取槽位；不运行Go测试或failpoint注入，proto复用锁定生成缓存。
这些是存储基线，**不是本任务源筛选Go运行oracle**。

已审查生产路径、注册、依赖提升、fixture provenance、阈值/顺序、故障、文档链接及
差异范围；整体和包级plan均未修改。初始任务78/80–88删除不纳入本任务提交。

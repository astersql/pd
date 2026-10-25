# 任务 002：基础集群观察契约

状态：已完成。批次 2；前置准入提交 `c09fd8334` 已核实。
Go 基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`，相关源码与基准一致。

## 生产边界

canonical crate `astersql-pd-schedule-core` 位于
[pkg/schedule/core](../../../pkg/schedule/core/Cargo.toml)。
[BasicCluster](../../../pkg/schedule/core/basic_cluster.rs) 提供 `stores()`、
`regions()` 借用及 store/region 查询、控制、范围和角色查询；直接为 canonical
`astersql_pd_core::BasicCluster` 实现，不复制状态。宿主仍通过真实
`RegionsInfo::check_and_put_region` / `atomic_check_and_put_region` 更新 root 与角色索引。
source、target 调度消费者得到同一 Arc payload。领域边界、错误与所有权说明见
[basic_cluster.rs.md](../../../pkg/schedule/core/basic_cluster.rs.md)。

根 workspace 只增加 member、集中路径依赖和本地 lockfile 包；没有远程版本变化。
离线 lockfile 生成曾自动选择 pulldown-cmark 0.13.5，已审查并恢复原锁定的
0.13.4；最终所有构建/验证均使用 `--locked`。

## 冻结 Go oracle

[fixture](fixtures/task_002.json) SHA256：
`368513b8fb4a0e0718292bd4c57fcd15ea8cbbb877b11461759b66c4d6f3388e`。
fixture 保存来源 commit、源码/二进制/输出 SHA256、输入和期望；来源、错误、
状态变更不从 Rust 结果生成。

[逐命令记录](logs/task_002/oracle-commands.json) 包含只读编译环境和精确 argv。
Go 编译只读现有测试为 Linux/ARM64 调试二进制；不运行生成器、make、
failpoint-ctl、toolexec 或模块整理。调试运行退出 0，但在断点取证后退出，
不把这两次运行当成完整原测试通过。

- [region.commands](logs/task_002/region.commands) 和
  [region 输出](logs/task_002/go-region-second.out)：复用
  `generateTestRegions(1000,5)` 的前两个真实对象，第二项的 ID/range 设为第一项、
  version 设为 101。实际观察 empty → version100/leader1/stores1,2,3 →
  version101/leader2/stores2,3,4；重放100返回输入 region 作为拒绝候选，
  缓存保持101。更新后 store1 count=0，store2/store4 count=1。
- [cluster.commands](logs/task_002/cluster.commands) 和
  [store 输出](logs/task_002/go-cluster-direct.out)：复用现有
  `TestEvictLeaderWithUnhealthyPeer` 的真实 core registry，显式以
  `tc.BasicCluster.StoresInfo` 调用，观察 missing999→nil/ErrStoreNotFound、
  pause-in→in=1/out=0、resume→in=0、重复 trend eviction→ErrSlowTrendEvicted、
  recover→0，以及 region stores={1,2,3}、followers={2,3}、leader=1。
  mockcluster 仅提供原测试的输入装配；观测和 Rust 测试均操作真实 core 对象。
- [既有断言运行](logs/task_002/go-safe-tests.out)：只读 allowlist
  `TestPutStore`、`TestPutStoreSkipsPatchForRemovedStore`、
  `TestUpdateRegionEquivalence`、`TestQueryRegions`、`TestStoreRegionCount`，
  5通过/0失败，命令退出0。TestQueryRegions 的 a→1、b→2、c→0（hole）
  作为 key lookup fixture；其余测试还验证真实更新和索引一致性。

逐路径检查了这些测试、helper、TestMain/goleak 和 core marker；它们不要求注入
failpoint，不运行 TestSetRegionConcurrence 等需要注入的案例。未把整个包放行裸跑。
调试器只构造输入、调用现有方法和读取状态，没有改写算法、错误或断言结果。
未使用持久化模拟：本任务是内存观察契约，不新建存储 backend 或 I/O 故障层。

fixture 只归一化无序 store 集合、nil/empty eviction slice 与调试器分配地址。
错误身份、拒绝候选、epoch、leader、store 成员及控制方向均参与比较。
保留旧 Arc 的不可变快照断言是 canonical core 的源码契约检查，单独标注，
不将其冒称为额外 Go 运行输出。未执行的随机与范围委托不声称有新增独立运行差分；
其实现委托 canonical core，并由本次全 workspace 基线覆盖既有测试。

失败取证尝试保存在 [attempts.json](logs/task_002/attempts.json) 和对应输出：
Rosetta 启动失败、注释行断点、core-only 链接裁剪、嵌入 receiver 返回值解码错误
均未用于 fixture；最终使用正确断点和显式 canonical receiver 的实际输出。

## 失败验证与通过证据

[初始失败](logs/task_002/red-compile.out)：精确聚焦命令退出101，缺失
BasicCluster 导出（零行为测试，不作为行为失败证据）。

实现后将 `get_region` 临时改为对所有存在的 ID 返回 None，仍运行同一组4个测试：
[行为失败](logs/task_002/red-behavior.out) 退出101，2失败/2通过；
`task_002_behavior_cases` 在真实插入后因 version/leader/store 断言失败，
`task_002_shared_source_target_views` 无法取得已存在 region。
[mutation.json](logs/task_002/mutation.json) 保存命令与结果；随后恢复实现。
补充 key hole 测试后，最终5项全部通过，无忽略或零测试替代。

| 测试 | 有效场景/观察数 | 结果 |
| --- | ---: | --- |
| task_002_behavior_cases | 4个冻结阶段 | 4成功/0失败 |
| task_002_shared_source_target_views | 2个更新阶段 | 2成功/0失败 |
| task_002_store_controls_and_errors | 5个控制转换 | 5成功/0失败 |
| task_002_membership_queries | 3个成员查询 | 3成功/0失败 |
| task_002_key_lookup_hole_and_boundary | 3个边界查询 | 3成功/0失败 |

这里17是表驱动场景/观察计数，Rust test harness 实际测试数为5。

[最终验证](logs/task_002/validation.json)、[汇总](logs/task_002/test-summary.json)：

| 确切命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| cargo fmt --all | 0 | 格式化 |
| cargo test -p astersql-pd-schedule-core --locked --lib task_002 -- --nocapture | 0 | 5通过/0失败/0忽略 |
| cargo test -p astersql-pd-schedule-core --locked | 0 | 5通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | workspace接线 |
| cargo fmt --all -- --check | 0 | 格式检查 |
| cargo check --workspace --locked | 0 | 编译检查 |
| cargo test --workspace --locked | 0 | 1317通过/0失败/1既有忽略 |

既有忽略为 `tso_processes`（需独立兼容进程脚本），没有新增 ignored 测试。
编译中已有的 proto deprecated field 警告不影响退出码。

## 槽位、只读检查与差异

所有 Cargo 阶段经 [slot-run.sh](logs/task_002/slot-run.sh) 原子 mkdir 动态领取：
实际均为槽位1，绝对 `CARGO_TARGET_DIR` 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
每次记录计划/任务/会话/PID，同步等待子进程退出，EXIT/INT/TERM 清理自有锁。
最终阶段的 [validation.out](logs/task_002/validation.out) 记录槽位及命令；
锁已释放，编译缓存保留。可复用入口为
`bash docs/rust/schedule-execution/logs/task_002/slot-run.sh python3 docs/rust/schedule-execution/logs/task_002/validate.py`。
日志展示版去除 EOF 空行及 shell 槽位行尾空格；[outputs.json](logs/task_002/outputs.json)
保留逐字节 Go 原始 stdout/stderr。fixture 的输出 SHA256 指该 JSON 中对应名称的
原始字符串，展示版路径供阅读；未删去任何错误或业务字段。

[开始快照](logs/task_002/before.json)、[检查脚本](logs/task_002/verify-go.py)、
[逐文件比较结果](logs/task_002/go-verification.json)：含 ignored/untracked 的
3012个 Go/模块文件，起止文件数相同、逐项SHA256相同、Git状态均为空。
聚合摘要均为 `b3b245160c15a1ac11a595607a62d6a735407ddd331becc495accf858c0da272`。
17份 plan.md 与准入快照哈希逐项相同，整体/包级 plan.md 和来源主题均未改写。
`git diff --check` 通过；审查仅本任务代码、局部接线、fixture与证据。

# 任务 004：调度器与检查器专用视图

批次 4，依赖批次 3 提交 `0eed403ec`；已核实其 canonical SharedCluster API、
[任务 003 证据](task-003.md) 和准入 `oracle-index.json`。准入中专用视图的运行
证据须在本任务补齐，不能以相关依赖已存在代替。

## 生产路径与契约

[pkg/schedule/core/cluster_views.rs](../../../pkg/schedule/core/cluster_views.rs)
提供 `SchedulerCluster`、`CheckerCluster`、`StoreConfigCluster`、`ClusterViews`
及 `BucketQueryError`。两类消费者通过各自 trait 使用同一个宿主视图，组合已有
SharedCluster、配置、store/bucket 统计和 label stats，没有复制规则或集群状态。
配置发布/借用代际、同步和错误边界见[源码说明](../../../pkg/schedule/core/cluster_views.rs.md)。

scheduler/store/checker 配置 getter 返回同一 canonical provider。
store loads 和 bucket 结果为 detached snapshot；bucket 使用 canonical worker 的
FIFO report/query 顺序，完整保留 region/key/degree/interval/六维 loads。
checker 排除 TiFlash 和缺失 store，用当前 location labels 观察每个 region，
在 batch 末尾清理 defunct，即使 batch 为空也清理。宿主负责 heartbeat overlap
产生的标记，专用视图不额外实现 heartbeat 流程。

局部接线只增加既有 buckets crate 和测试 Registry 依赖，tokio-util 从 dev
提升为普通依赖；Cargo.lock 只增加本地 schedule-core 包的两条依赖，无远程版本变化。
没有新 features 或 server/runtime 反向依赖。

## 冻结 Go oracle

[fixture](fixtures/task_004.json) 保存独立于 Rust 实现的输入/输出。
[provenance](logs/task_004/provenance.json) 保存来源 commit、二进制/dlv SHA256、
已有 cluster 二进制的编译记录引用、本任务 bucket 只读编译的精确 argv/env。
[sources.json](logs/task_004/sources.json) 核实实际参考的 Go 路径与
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 相同。

- [label.commands](logs/task_004/label.commands) 与
  [label.json](logs/task_004/label.json)：既有 `TestRegionLabelIsolationLevel`
  创建真实 core/stats，stores 1/2/3 分处 zone1/2/3，store4 为 zone3/TiFlash。
  原测试实际观察 label counter zone=1→2→1，最后一步包括 heartbeat overlap
  标记和 checker 更新清理。调试器另外调用 canonical RollingStoreStats 输入 setter，
  观察 12 维 store loads 更新到 region write bytes/keys=30/40，移除 store1 后
  只剩 2/3/4。初始状态已有四个 store，而非空 map。
  三类配置 getter 指向同一 PersistOptions；max replicas 3→5、max merge size
  54→7、scheduler diagnostic true→false，store max size=144。
  `CheckRegionSize(200,20)` 返回 `PD:checker:ErrCheckerMergeAgain` 的 size 原因，
  `(150,20)` 返回 nil。完整错误在调试器 max-string-len=1024 下保存。
- [buckets.commands](logs/task_004/buckets.commands) 与
  [buckets.json](logs/task_004/buckets.json)：既有 `TestCheckBucketsTask`
  真实 worker/caches。region1 的 10–20、20–30 两个 bucket 均为 degree=-1、
  interval=1000、六维 loads=0；degree=0 无结果。随后 region2 的 20–30
  继承为 degree=-2；选 region2 得到该项，选999为空。
  调试器借用已有报告的 u64 slice 作为查询输入，仅在报告已被处理后将其设置为
  [2] 或 [999]，不改动缓存、算法、断言或返回值。

[attempts.json](logs/task_004/attempts.json) 保留变参调用与注释行断点的早期失败，未用于 fixture。
上述最终断点采集退出0，只计实际方法观察，不冒称完整原测试通过。
[safe-tests.json](logs/task_004/safe-tests.json) 是另外完整运行原有
`TestRegionLabelIsolationLevel`、`TestCheckBucketsTask`、
`TestCollectBucketStatsTask` 的输出：3通过/0失败，两个命令均退出0。
逐条检查所选测试、helper 和 TestMain，不需要 failpoint 注入；没有放行整个 Go 包。
cluster 测试的 mock ID allocator 仅用于原有宿主装配，没有将其分配输出用于 oracle；
Go 与 Rust 的受测状态、统计和配置均来自真实 canonical 对象。

归一化仅涉及 map 顺序、ASCII keys、调试器地址及 stack；store loads 在 JSON
统一为 float64，所有12维参与比较；bucket 全字段、label 数值、错误分类和配置值
均保留。旧 snapshot 保留、空 batch 清理以及 Rust worker 取消/关闭另标为扩展
检查，不冒充额外 Go 运行差分。Go map 返回没有 Rust `BucketQueryError` 等价返回；
该类型原样组合既有 worker 的 admission/wait 错误。

## 失败验证与真实行为测试

[red-compile.out](logs/task_004/red-compile.out) 是同一聚焦命令退出101，缺少本任务
5个导出 API；零行为测试，仅作初始缺失证据。初次测试装配的字段名、JSON int/f64
及未调整配置代际问题在后续验证前修正；相关日志保留，不作为业务失败证据。

[mutation.py](logs/task_004/mutation.py) 在实现后逐项引入错误并在 finally 恢复源码、
执行格式化；每次仍运行同一组已注册的3项 task_004 测试。
[mutation.json](logs/task_004/mutation.json) 保存命令、槽位、退出码和有效计数：

| 错误行为 | 失败测试 | 有效结果 |
| --- | --- | --- |
| store 查询固定空 map | task_004_behavior_cases | 2通过/1失败，退出101 |
| bucket degree 强制至少0 | task_004_behavior_cases | 2通过/1失败，退出101 |
| 不排除 TiFlash | task_004_checker_label_updates | 2通过/1失败，退出101 |
| 不清理 defunct | task_004_checker_label_updates | 2通过/1失败，退出101 |

每次均为实际状态/结果断言失败，不是缺包或编译失败。

| 最终测试 | Go 冻结观察与额外边界 |
| --- | --- |
| task_004_behavior_cases | 3个store map阶段；4个bucket筛选结果；额外旧snapshot保留和canonical指针身份 |
| task_004_checker_label_updates | 3个label counter阶段，包含TiFlash排除、重复observe后defunct清理；额外空batch清理 |
| task_004_config_and_typed_faults | 2代配置；每代重复检查2个真实store尺寸输入；真实worker取消/关闭的2类类型化错误 |

Rust harness 为3项测试；独立 Go 冻结观察共14组（3+4+3+2+2），不把重复执行
或指针断言计成独立 Go 案例。持久化仅复用真实 MemoryStore 装配规则/ID owner，
本任务没有新增持久化后端或注入规则存储错误。

## 最终验证

状态：已完成。fixture SHA256：`e4ed75e169dd44ddd795ae058e0054b7e8b878fabb2e4584da13a9eae7b17501`。

[validation.json](logs/task_004/validation.json) 保存精确 argv、退出码、有效测试数量；
对应 `validation-0.out` 至 `validation-6.out` 保存 stdout/stderr。
最终测试还显式证明同一个存活视图读到 store loads 更新，而旧返回值独立保留；
scheduler 专用 getter 读取 diagnostic 的 true→false，不仅比较共同配置字段。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-core --locked --lib task_004 -- --nocapture` | 0 | 3通过/0失败/0忽略 |
| `cargo test -p astersql-pd-schedule-core --locked` | 0 | 14通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace接线 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | 编译检查 |
| `cargo test --workspace --locked` | 0 | 1326通过/0失败/1既有忽略 |

既有忽略为 `tso_processes`，需独立进程兼容脚本；没有新增忽略。
既有 proto deprecated 字段警告没有影响退出码。所有 `.out` 展示文件只进行
ANSI/空白展示规范化，Go 原始 stdout/stderr 保存在 JSON，fixture 引用 JSON 哈希。

所有 Cargo 阶段均由 [slot-run.py](logs/task_004/slot-run.py) 动态原子领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁中记录 plan/task/session/PID，子命令继承绝对路径；正常、失败和 INT/TERM
清理自有锁，等待自身子进程结束后释放，不删除其他任务锁或缓存。
最终验证阶段全部退出0，自有锁已释放，槽位缓存保留。

[verify.py](logs/task_004/verify.py) 对照 [before.json](logs/task_004/before.json)
核对包括 ignored/untracked 在内的3012个 Go/模块文件，起止逐项SHA256和Git状态
完全相同；摘要为 `b3b245160c15a1ac11a595607a62d6a735407ddd331becc495accf858c0da272`。
17份只读 plan.md 的哈希相同，整体/包级计划和来源主题没有改写。
[go-verification.json](logs/task_004/go-verification.json) 保存逐项比较结论；
[evidence-verification.json](logs/task_004/evidence-verification.json) 校验 fixture
原始运行输出哈希、来源哈希、复用二进制哈希、3项完整Go测试、4次行为失败和最终
有效计数，以及仅本地 lockfile 依赖变更和文档链接。
`git diff --check` 与暂存差异检查通过，只提交本任务源码、接线、测试、fixture和证据。

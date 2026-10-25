# 任务 003：共享调度规则与分配契约

批次 3；依赖批次 2 的提交 `3717af713`、canonical BasicCluster API、
[运行差分与基线证据](task-002.md) 已核实。Go 源码基准
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`。

## 生产路径与范围

[pkg/schedule/core/shared_cluster.rs](../../../pkg/schedule/core/shared_cluster.rs)
导出 `SharedCluster`、`SharedClusterView`、`SharedIdAllocator`、`IdAllocation`、
`PrepareRegionCount`。全部组件引用宿主已有 canonical 实现；没有第二套状态缓存。
配置、BasicCluster、四类规则 owner、statistics 的共享和所有权边界见
[源码说明](../../../pkg/schedule/core/shared_cluster.rs.md)。

classic 分配忽略输入 count，counted 分配保留 count，均委托真实持久化 ID
分配器。远端 ID/prepare 服务保留关联错误类型，共享层不重试或吞掉错误。
HTTP/gRPC 客户端、宿主生命周期和真实 unsafe recovery 驱动留在后续适配任务。
这些边界没有用返回成功的桩代替生产实现。

Cargo 仅扩充本包对既有 workspace 包的依赖。离线生成 lockfile 后只保留
schedule-core 本地包的依赖列表，所有远程版本与任务开始时逐字一致。

## 冻结 Go oracle

[fixture](fixtures/task_003.json) 保存固定输入/输出、逐字段归一化和运行记录哈希。
[compilation.json](logs/task_003/compilation.json) 保存编译 argv、环境、二进制 SHA256；
[sources.json](logs/task_003/sources.json) 核实19个实际读取源码路径与基准相同。
Go 只读编译现有测试，未运行生成器、格式化、模块整理或 failpoint 注入。
调试器调用原有方法、装配输入和读取真实返回值，断点处退出；不冒称完整原测试通过。

- [shared.commands](logs/task_003/shared.commands)、
  [原始 stdout/stderr 与命令](logs/task_003/shared-final.json)：复用既有
  `TestAskBatchSplitRejectsAffinityAutoSplit` 的真实调度集群，观察未加载规则为nil、
  label/affinity miss、keyrange empty、halt false→true→false、prepare URL错误、
  缺少 PD leader 的 ID错误；原测试创建 g1 后观察 pending/版本2/范围1及 labeler
  的 `affinity_group/g1` 和 region label g1。keyrange 非空输入由 canonical
  KeyRanges.Append/SortAndDeduce 装配（与 public Manager.Append 的内部方法一致），
  然后调用 public Manager 查询，观察 false 与完全覆盖时空剩余范围。
- [rules.commands](logs/task_003/rules.commands)、
  [原始输出](logs/task_003/rules-run.json)：既有
  `TestEvictLeaderWithUnhealthyPeer` 装配真实规则管理器，观察
  pd/default voter/count3、pd/missing=nil；BasicCluster getter 与宿主地址一致。
  mockcluster 仅用于既有测试输入装配；不采用其 mock ID/prepare 返回值作为生产 oracle。
- [id.commands](logs/task_003/id.commands)、[原始输出](logs/task_003/id-final.json)：
  既有 `TestIDAllocationEndValue/KeyspaceLabel...`，真实独立 embedded etcd 和
  canonical ID allocator。base16777212、step1，count0→(16777212,0)、
  count2→(16777214,2)、count1→(16777215,1)，再count1返回
  `PD:idalloc:ErrIDExhausted`。这是 classic keyspace 上限；不声称额外覆盖 nextgen。
- [classic.commands](logs/task_003/classic.commands)、
  [原始输出](logs/task_003/classic-final.json)：既有 `TestStoreHeartbeat` 在放入
  newTestRegions(3,3,3) 后，真实 RaftCluster.GetPrepareRegionCount 返回3,nil，
  BasicCluster getter 与宿主地址一致。未采用其中 mock allocator 的返回值。

这些路径的测试和 helper 不需要 failpoint 注入分支；没有裸跑需要注入的测试。
初始字段名、并发断点重入和空行断点取证失败保存在
[attempts.json](logs/task_003/attempts.json)，没有用于 fixture。

归一化仅涉及调试器地址、nil/empty集合和宿主提供的创建时间；g1的phase、version、
label/range、ID/count和错误内容均参与比较。Go错误分类映射为已有
AllocatorError::Exhausted。prepare测试在外部HTTP I/O边界注入实际冻结的
operation/url/cause，remote ID同理注入真实缺少leader错误；断言同一Arc错误对象
被原样传播。这证明领域委托与错误传播，不宣称实现了HTTP/RPC客户端。
classic固定count1和recovery OR另标为源码契约；canonical Alloc(1)有运行输出，
不把映射检查或Rust lease故障冒称为新增Go端到端运行差分。

## 失败验证与行为测试

[red-compile.out](logs/task_003/red-compile.out)：同一聚焦命令退出101，缺失4个共享API；
有效行为测试0，仅记录初始缺失。实现后的
[mutation.json](logs/task_003/mutation.json)、[red-behavior.out](logs/task_003/red-behavior.out)
将halt返回值改为false，同一6项测试5通过/1失败，退出101；失败为
`task_003_halt_and_prepare_error` 中false≠true。脚本finally恢复源码并重新格式化。

| 测试 | 实际场景与证据边界 |
| --- | --- |
| task_003_behavior_cases | 默认规则与miss、label/affinity miss、创建g1及label/region查询、keyrange空/非空/覆盖；canonical指针身份与两个消费者同一owner |
| task_003_halt_and_prepare_error | 3个Go halt输入；1个冻结prepare错误；附加recovery OR和空本地count源码检查 |
| task_003_id_allocation_success_and_failure | 3个Go成功count输入+1个耗尽错误；附加失败后base保留检查 |
| task_003_classic_count_and_leadership_fault | 2个classic count映射源码检查+真实MetaStore lease撤销故障；不使用mock分配器 |
| task_003_local_prepare_count_uses_canonical_regions | 1个真实Go count=3结果，Rust同范围与ID的canonical region插入 |
| task_003_remote_allocation_error_is_preserved | 1个冻结Go missing-leader错误，原样传播Arc |

没有忽略本任务测试；Rust harness计数为6。表驱动部分明确为3个halt输入和
3个成功ID输入，ID耗尽和lease撤销均是额外失败场景。

## 最终验证与只读检查

状态：已完成。fixture SHA256：`26e4da914e03759c0360cf28638f07323aee2f8fdaa00195cfe89f79b5506cb7`。

[validation.json](logs/task_003/validation.json) 保存精确argv、退出码及harness计数；
[final-focus.json](logs/task_003/final-focus.json) 在最终fixture provenance更新后重新运行聚焦验证，6通过/0失败。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 检查 |
| `cargo test -p astersql-pd-schedule-core --locked --lib task_003 -- --nocapture` | 0 | 6通过/0失败/0忽略 |
| `cargo test -p astersql-pd-schedule-core --locked` | 0 | 11通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查 |
| `cargo fmt --all -- --check` | 0 | 检查 |
| `cargo check --workspace --locked` | 0 | 检查 |
| `cargo test --workspace --locked` | 0 | 1323通过/0失败/1忽略 |

既有1项忽略为tso_processes，需独立进程兼容脚本；没有本任务新增忽略。
已有proto deprecated字段警告不影响退出码。stdout/stderr保存在对应运行记录和日志；
展示用.out去除ANSI、展开tab和行尾空格，原始Go stdout/stderr仍保留在JSON，
fixture哈希引用JSON，不引用展示归一化文件。

全部Cargo阶段动态领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[slot-run.sh](logs/task_003/slot-run.sh) / [slot-run.py](logs/task_003/slot-run.py)
通过原子mkdir领取1–10中的首个空闲槽，记录计划/任务/会话/PID；同步等待子进程。
最终封装将Cargo命令放入独立进程组，INT/TERM先终止并等待自己的进程组，再释放锁。
正常/失败路径均在finally清理自有记录；所有实际运行正常结束，自有锁已释放，缓存保留。

[go-verification.json](logs/task_003/go-verification.json) 对照
[before.json](logs/task_003/before.json)，3012个Go/模块文件（含ignored/untracked）
逐项SHA256相同，Git状态均为空；聚合摘要均为
`b3b245160c15a1ac11a595607a62d6a735407ddd331becc495accf858c0da272`。
17份只读plan.md逐项哈希相同，未修改原始主题。
`git diff --check`和暂存差异检查通过；审查仅本任务Rust代码、局部接线、锁文件、fixture和证据。

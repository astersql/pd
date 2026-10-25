# 任务053：拆分散射观察状态

状态：已完成。

生产路径：[split_scatter_observe.rs](../../../pkg/schedule/checker/split_scatter_observe.rs)，
[领域 API 和接线边界](../../../pkg/schedule/checker/split_scatter_observe.rs.md)。
复用 canonical checker、core RegionsInfo、codec、任务52 range hint，无新增依赖、
Cargo crate 或 feature。后继任务54负责调度、退避、容量、过期和指标策略。

## 依赖及来源

批次52提交 `7b680e69c` 和 [任务52证据](task-052.md) 已核实，相关82个分组/前缀
案例纳入本次 checker 包级验证。已读取整体/包级只读计划、ADR-001、依赖基线、
移植指南及 oracle-index.json。索引没有本任务 Go 运行输出；按用户后续授权，
**Go 源码与测试契约推导 + Rust 行为验证**满足准入，没有运行 Go oracle 或声称运行差分。
来源 commit、源码哈希和固定 fixture 哈希见
[source-provenance.json](logs/task_053/source-provenance.json)，Go 输出哈希为 null。

`split_scatter.go` 的 `splitScatterResultReady` / `splitScatterSourceObserved` 定义版本门限，
`recordSplitScatterBatch` 定义 group 和请求版本，`samePendingSplitScatter` 定义身份。
`hasSplitScatterTxnKeyspaceBounds` 和 `keyspace.MakeRegionBound` 定义精确 Txn 边界。

只读审阅的 Go 测试覆盖关系：

- `TestRecordSplitScatterBatchCollectsPendingRegions`：登记 source/children、共享 group、完整观察。
- `TestDispatchSplitScatterKeepsPendingUntilSplitHeartbeat`：结果存在仍须等待 source heartbeat。
- `TestDispatchSplitScatterVersionLifecycle`：低版本等待、高版本 stale、较新 source 不阻塞 sibling。
- `TestRecordSplitScatterBatchDoesNotBumpPastRequestSplitVersion`：保存请求版本。
- `TestDispatchSplitScatterIgnoresStalePendingSnapshot` / `...WithSameGroup`：group/deadline 身份。
- `TestResolveSplitScatterRangeHintRequiresKnownTxnKeyspaceBounds`：两个已知精确 Txn 边界。
- `TestSplitScatterControllerCleanupResetsPendingGauge` 属后继回收/指标；
  `TestCheckSplitScatterRegionsCreatesScatterOperator` 属 operator 派发，不把名称当成本次已验证行为。

## 行为与失败验证

[固定输入/期望](fixtures/task_053.json) 是独立于 Rust 实现按上述只读源码推导的13组
结果/source 版本与缺失组合。null 表示缺失 region，-1 表示存在但没有 epoch。
时间以 caller-supplied monotonic Duration 映射 Go deadline，只比较精确相等；
没有归一化忽略错误、group、version 或任何状态。confver 不参与观察就绪判定。

[task_053_test.rs](../../../pkg/schedule/checker/task_053_test.rs) 显式注册4个真实行为测试：
13组固定 gate；部分结果/source 再次拆分及重复批次生命周期；
5个 keyspace（0、1、255、65535、最大24位 ID）各3组边界状态及5组真实分组集成；
5个独立身份字段变更与相同身份重复登记。使用真实 RegionInfo、RegionsInfo、codec，
比较实际 pending/ready/删除结果，不使用 mocks。

先写并注册测试，观察 gate 暂时返回 Ready；[red.log](logs/task_053/red.log) 退出101，
3个测试中2个实际行为断言失败（缺失结果误就绪、未观察 source 提前就绪），不是编译错误。
实现后相同测试通过，随后新增独立身份字段覆盖。
[mutation.py](logs/task_053/mutation.py) 逐项破坏 source >= 门限、stale gate、deadline 身份、
双边界校验；[变异结果](logs/task_053/mutation-results.json) 四项均退出101并触发实际断言。
脚本 finally 恢复正常实现；最终验证在恢复后运行。
无持久化/I/O 行为，因此不引入 MetaStore 或模拟持久化故障；故障为真实 registry
缺失、旧 heartbeat、新 heartbeat、旧批次快照及损坏判断条件。

## 验证与共享槽位

[run.py](logs/task_053/run.py) 为每条 Cargo 命令原子 mkdir 领取槽位1–10，
记录 owner PID/任务/会话；所有 Cargo 子进程继承绝对 CARGO_TARGET_DIR，正常/失败/
中断等待子进程组退出再清理自己的锁，保留编译缓存。完整命令/退出码在每个阶段 JSON，
完整输出保存在同名 log。Rust 修改后先执行 cargo fmt --all，再运行验证。

最终各阶段实际使用槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_053 -- --nocapture` | 0 | 4通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 33通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |

workspace 首轮和限制测试并发为2的首轮均退出101，前者为 etcd WAL 写入 ENOSPC，
后者为 Go fixture linker ENOSPC；[首轮 fixture 日志](logs/task_053/etcd-failure.log) 保存
底层原因。不将环境失败算作本任务行为成功。已确认持有 PID 不存在后，在领取的槽位中
清理生成的临时 etcd 数据/辅助二进制，释放约3GB，没有删除 Rust 编译缓存、源码或锁；
[清理清单](logs/task_053/fixture-cleanup.json) 记录路径和原进程标识。随后使用
`RUST_TEST_THREADS=2 cargo test --workspace --locked` 重新验证完整 workspace。

## 差异审查

[audit.json](logs/task_053/audit.json) 核对1101个 Go/模块文件的完整文件清单、SHA256
和 Git 状态，与开始相同。两个只读 plan.md 内容未变，文档相对链接有效，
`git diff --check` 通过；只提交本任务 canonical 模块/注册、测试、fixture 和证据。
保留并排除其它任务编号 Markdown 的既有删除。无 Go make、failpoint、生成/格式化写入。
workspace 基线的既有辅助程序只读构建 Go fixture，不作为本任务 Go oracle。

后续完整重跑记录保留：`workspace-test-final`（并发2）在既有
`rule_endpoint_test::rule_endpoint_etcd` 的一次 save 返回 Unavailable；
同一测试单独重跑 `storage-endpoint-recheck` 退出0，1通过。
`workspace-test-serial`（并发1）完成1468通过、1失败、1既有忽略，失败为既有
`real_etcd_oracle_process_restart` 查找子进程 stdout 的 `PROCESS_TSO ` 行。
`tso-recheck` 并发1单独重跑复现相同失败；该测试使用当前 exe 的 `--nocapture` 子进程，
并发1会改变 libtest 的行前缀。未修改该范围外测试；恢复前序任务采用的并发2重验。


## 最终结果

`tso-normal-threads` 并发2单独重跑退出0、1通过。
最终 `RUST_TEST_THREADS=2 cargo test --workspace --locked` 退出0，
**1491通过、0失败、1既有忽略**（1489个运行测试、2个 compile-fail 文档测试）。
见 [workspace-test-recovered.json](logs/task_053/workspace-test-recovered.json)、
[完整输出](logs/task_053/workspace-test-recovered.log) 和
[汇总](logs/task_053/test-summary.json)。既有忽略为 `tso_processes`，未新增忽略。
全部必需命令最终通过；前述异常完整保留，不作为成功计数。
[命令汇总](logs/task_053/commands.json) 包含各 Cargo 阶段实际命令、退出码、槽位和环境。
全部子进程已退出，自己的槽位锁已释放，Rust 编译缓存保留。
日志仅移除行末空白和末尾空行，以满足差异检查；不改写错误或测试输出内容。

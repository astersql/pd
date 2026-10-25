# 任务052：拆分散射分组键

状态：已完成。

生产实现：[split_scatter_group.rs](../../../pkg/schedule/checker/split_scatter_group.rs)，
[API 与领域边界](../../../pkg/schedule/checker/split_scatter_group.rs.md)。
公开 `SplitScatterRangeHint`、`resolve_split_scatter_range_hint`、
`split_scatter_prefix_range_with_keyspace_group`、`split_scatter_next_prefix`。
只接入 canonical checker 模块；不新增 crate、feature、依赖或修改 Cargo.lock。

## 来源与依赖

依赖批次51由提交 `7a6848e71`、[任务51证据](task-051.md)、实际
`PriorityInspector` API 和本次 checker 全量测试确认。已读取只读整体/包级指南、
ADR-001、依赖基线、移植指南及 oracle-index.json。该索引没有本任务的 Go 运行输出。
按用户明确授权，本任务依据**只读 Go 源码及测试契约推导 + Rust 行为验证**验收；
没有执行 Go checker oracle，没有声称运行差分，Go 输出哈希为 null。

来源 commit、各文件哈希及最终 fixture 哈希见
[source-provenance.json](logs/task_052/source-provenance.json)。直接依据
`split_scatter_group.go` 的分组解析、范围构造及 successor；编码依据
`codec.go` 的 EncodeBytes/DecodeBytes、EncodeInt/DecodeInt、表/索引和 keyspace 前缀。
`split_scatter_test.go` 的 `TestCollectTopPendingResolvesRangeHint` 六组案例、
`TestResolveSplitScatterRangeHintIgnoresRawLikeKeyspaceKeys` 及
`TestResolveSplitScatterRangeHintRequiresKnownTxnKeyspaceBounds` 是已有测试契约。
`SystemKeyspaceID` 和 `MaxValidKeyspaceID` 取自只读 keyspace constant 源码。
超出已有 Go 测试的边界期望直接从生产源码推导，未伪造 Go 运行结果。

## 行为与失败证据

[fixture](fixtures/task_052.json) 冻结75个分组案例和7个前缀案例；
[task_052_test.rs](../../../pkg/schedule/checker/task_052_test.rs) 显式注册两个测试，
使用真实 RegionInfo、canonical codec 和生产公开 API。初始预期在实现前由独立
Python 字节编码按上述 Go 源码推导；补充案例沿用同一推导，未调用待测 Rust 生成输出。
先冻结69组，再补充 SystemKeyspaceID 的6组案例；前缀案例始终为7组。
逐字节检查 start/end 及完整 group 字符串，唯一表示映射为 Go nil/空字节串对应
Rust 空 Vec；无时间、随机或错误字段归一化。

覆盖 classic/txn/raw、默认/普通/system/max keyspace，已知/拒绝/缺失验证器、
表/索引/记录/裸边界、索引排他上界相等与越界、跨表/实体/keyspace、无穷终点、
正负与 i64 极值、短整数、短 keyspace、无效 mode、非表 payload、空/短/错误 marker/
错误 padding/缺终止分组、允许编码后缀与不解码终点；空/全ff/连续ff进位与截断。

[red.log](logs/task_052/red.log) 为 RegionMeta 字段名不匹配的编译失败，不计行为证据。
修正字段后，[red-behavior.log](logs/task_052/red-behavior.log) 证明空行为实现使两个
注册测试实际断言失败（索引分组与前缀范围）；实现后同一测试通过。

[mutation.py](logs/task_052/mutation.py) 逐项临时引入错误并 finally 恢复生产文件，
每项先 fmt，再用同一聚焦测试验证。预期检出：上界相等错误回退、跳过 keyspace
验证、短索引错误退回表组、无穷索引错误保留索引组、prefix进位后漏截断。
结果见 [mutation-results.json](logs/task_052/mutation-results.json)。无外部 I/O 或
持久化写入；损坏输入通过真实编码解码路径验证，不需要 mock/MetaStore 故障注入。

## 验证

[run.py](logs/task_052/run.py) 为每个阶段顺序原子 mkdir 领取共享槽位1–10，记录
计划/任务/会话/PID，所有 Cargo 子进程继承绝对 CARGO_TARGET_DIR；正常、失败、
中断均等待子进程组退出后释放自己的锁并保留缓存。确切命令及退出码保存于同目录
JSON/log。最终工作区测试采用 `RUST_TEST_THREADS=2`。

## 差异审查

Go/模块与所有 plan.md 按开始/结束全量文件哈希、文件清单及 Git 状态核对。
不运行 Go 测试、make、failpoint 或 Go 写入命令。既有 workspace 验证可能在槽位中
只读构建 Go helper，不构成本任务 oracle。保留并排除其它编号任务文件的已有删除。


所有阶段实际槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
命令、退出码与环境汇总见 [commands.json](logs/task_052/commands.json)。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_052 -- --nocapture` | 0 | 2测试通过，75分组+7前缀案例 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 29通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1487通过、0失败、1既有忽略 |

五项变异均exit101且实际行为断言失败，正常实现已恢复。既有忽略项为
`tso_processes`，没有新增忽略。既有 server 的 deprecated CPU 字段警告不影响通过。
所有子进程已结束，自己的槽位锁已释放，缓存保留。日志仅清理行末空白和末尾空行。
[audit.json](logs/task_052/audit.json) 核对1101个 Go/模块文件的内容、完整文件清单及
Git状态、18份只读plan哈希、文档相对链接和差异检查；均通过。

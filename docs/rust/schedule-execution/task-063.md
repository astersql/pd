# 任务063：拆分执行与结果扫描

前置任务62由提交 `01ae53a20` 和 [task-062](task-062.md) 核实：canonical
`RegionGroupKeys` / `group_keys_by_region`、真实 replication/placement 契约、
2项包测试及workspace成功证据已就绪。本任务独占批次63，无并行代理。

生产路径：[split_results.rs](../../../pkg/schedule/splitter/split_results.rs)。
公开 `split_region_by_keys`、`SplitExecutionError`、`SplitKeyResults`、
`SplitGroupProgress`、`SplitAttempt`、`SplitSubmissions`。领域边界与宿主接线见
[说明](../../../pkg/schedule/splitter/split_results.rs.md)。宿主将成功准入的
`AdmissionOutcome` 接入既有 dispatch；本任务不新增 transport 或 runtime。
真实 heartbeat split command 的 policy/keys 在测试中逐字段核对。

遵循用户后续授权，证据为**只读Go源码/测试契约推导 + Rust行为验证**，未运行Go，
没有声称Go/Rust运行差分。来源、commit、SHA256、空binary/output hash和逐文件
基准比对保存于 [provenance.json](logs/task_063/provenance.json)。已核查
[oracle-index](oracle-index.json)，历史运行案例不覆盖本任务，未改写索引。
来源Go均与计划基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe` 相同。

冻结契约：

- `region_splitter.go::SplitRegionByKeys`：真实工厂创建 description
  `region-splitter` / OpSplit / USEKEY，交给 controller；构建与准入失败不能计成功。
- `splitRegionsByKeys`：仅成功提交的组被扫描；finished组跳过；部分观察结果也保留。
  `ScanRegionsByKeyRange` 用原范围扫描，按 start-key 字节相等记录ID；不要求
  ID改变或epoch增加，不重新做leader/复制检查。仅epoch增长不能产生结果。
- `splitKeyResults`：ID→start跨tick覆盖/累积；unprocessed按输入顺序保留所有未
  观察key的重复项。finished用当次扫描的唯一ID数量比较原始keys数量，因此
  重复key或跨tick累积所有key不等于组finished。原ID可作为右子region被记录。
- Go `TestMain` 是环境入口；`TestRegionSplitterTestSuite/TestRegionSplitter`
  覆盖fff/ggg成功、aaa/bbb不存在及比例，但使用mock handler，不能证明真实扫描。
  `TestGroupKeysByRegion` 覆盖分组。它们没有Go运行证据，也不足以代替本任务
  的真实Rust controller/core扫描测试。未裸跑failpoint相关Go测试。

[fixture](fixtures/task_063.json)有9个固定原始输入/期望案例：未完成、epoch-only、
部分、全部、原ID为右子region、重复keys、空扫描、排他end和二进制不匹配。
另6个独立行为测试覆盖真实controller准入与heartbeat命令、重复准入拒绝、
过期epoch拒绝、joint状态构建拒绝、跨tick累积/锁存、真实分组→提交→更新core索引
→扫描并标记suspect、拒绝组不扫描、无界二进制匹配和同ID覆盖。
有效测试7项，固定fixture9场景全部执行；额外6个测试按各自断言验证。
组/结果按ID排序归一化Go未指定的map顺序，不忽略keys、epoch、错误或状态差异。

使用真实 `BasicCluster/RegionsInfo`、`ControllerAdmission`、工厂、heartbeat领域命令、
replication filter；RuleManager基于真实MemoryStore/StorageEndpoint构造，legacy分支
不需要规则初始化。无mock业务判定、无生产故障开关，无持久化副作用。本任务故障
是joint构建失败、epoch过期/已有操作准入拒绝、未观察/部分观察；不宣称存储或网络
故障覆盖。suspect仍由批次62的复制校验产生，扫描不另行通知。

TDD记录：

- [red.log](logs/task_063/red.log)：首轮命令因heartbeat领域类型使用错误退出101，
  仅作为编译修正记录，不计行为失败证据。
- [red-behavior.log](logs/task_063/red-behavior.log)：修正测试编译后，临时空扫描
  实现导致同组扫描测试与partial累积测试发生具体断言失败，2通过、2失败、退出101。
  临时实现已替换。这证明同一测试可识别错误行为，而非只证明API缺失。
- [focused.log](logs/task_063/focused.log)：最终7通过、0失败，9条fixture全部执行。
- [package.log](logs/task_063/package.log)：含前置62共9通过、0失败，零doctest不计有效测试。

Cargo由 [run.py](logs/task_063/run.py) 在每个阶段原子mkdir动态领取仓库共享槽位
1–10；owner记录计划/任务/会话/PID，所有子进程继承绝对CARGO_TARGET_DIR。
中断先终止/等待子进程组再释放自己的锁，保留缓存。每次Rust修改后先cargo fmt --all。
[lock.log](logs/task_063/lock.log)是允许的离线锁文件生成；审查后撤回附带的
pulldown-cmark版本漂移，最终Cargo.lock仅增加两个已有workspace路径依赖。
其余检查全部使用--locked，远程版本/校验和保持不变。

最终验证均退出0，完整stdout/stderr和实际命令在各log及同名JSON中：

| 命令 | 结果 |
| --- | --- |
| `cargo test -p astersql-pd-schedule-splitter --locked --lib task_063 -- --nocapture` | 7通过、9条fixture全部执行 |
| `cargo test -p astersql-pd-schedule-splitter --locked` | 9通过、0失败 |
| `cargo metadata --format-version 1 --no-deps` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过 |
| `CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 1545通过、0失败、1既有忽略 |
| `git diff --check` | 通过 |

[workspace完整日志](logs/task_063/workspace-test.log)、[计数](logs/task_063/counts.json)
包括2项compile-fail doctest；既有tso_processes默认忽略不计成功。唯一既有
`server/core_region_adapter.rs:134 cpu_usage` deprecated警告保留，无新增代码警告。
所有阶段动态领取的实际槽位为**1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
全部子进程已经退出，自己的槽位锁已释放，缓存保留。

[审计](logs/task_063/audit.json)对照[初始清单](logs/task_063/before.json)：1101个
Go/模块文件的内容哈希、清单、Git状态不变；两个只读plan.md不变；无范围外Rust
格式化变更。Cargo.lock远程版本/校验和不变，仅本crate增加两个已有路径依赖。
Markdown相对链接已检查。日志只清理行尾空格和末尾空行，不改变运行输出语义。
既有其它任务删除不纳入本任务提交；完成后按要求删除任务063编号文件。

状态：已完成。

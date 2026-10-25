# 任务061：批量散射重试与取消

状态：已完成。

前置批次60提交 `077cfd865`，见 [task-060](task-060.md)：真实目标选择、
admin操作工厂与分组记账均可调用，本次包级12测试包含前置59/60的8测试。
生产路径 [scatter_batch.rs](../../../pkg/schedule/scatter/scatter_batch.rs)，
[边界说明](../../../pkg/schedule/scatter/scatter_batch.rs.md)。公开入口
`ScatterBatch::scatter` 接收 `ScatterBatchRequest::{Ids,Range}`，返回准确的
`processed/failures/pending/suspect/cancelled`；类型化错误保留 source。

遵循用户后续授权：采用**只读Go源码/测试契约推导 + Rust行为验证**，未运行Go，
没有Go运行oracle或运行差分。源码/fixture SHA256与完整来源commit保存在
[provenance.json](logs/task_061/provenance.json)。Go output/binary hash为null。
已核查 [oracle-index](oracle-index.json)，历史internal failure-accounting不能代替
本次admin批处理证据。两份plan.md只读，未更改历史oracle索引。

来源语义核实：

- `region_scatterer.go:480–590` 给出空范围/ID、单个missing与混合missing、
  负重试、30上限、指数退避、最终失败轮也sleep、失败清理和计数语义。
  成功Scatter调用即增加opsCount；Controller拒绝仍计数并退出重试集合。
- `604–676` 给出replication/suspect(false)、已有同组/epoch、leader、affinity、hot顺序；
  `825–876` 给出真实工厂、HIGH/group元数据及无操作/成功/失败指标。
  replica数量异常产生suspect标记；pending peer本身并不构成admin复制不足。
- `TestMain` 仅环境入口；`TestScatterRegions` 是单区目标/操作契约；
  `TestStoreLimit` 使用AddWaitingOperator而非批处理AddOperator，不是批重试oracle；
  `TestScatterCheck` 核实replica数量及PendingProcessedRegions标记。
  `TestSomeStoresFilteredScatterGroupInConcurrency` 明确仅防panic，不验证散射结果。
  另核实`TestScattersGroup`中scatterFail/Drain依赖failpoint的失败映射契约，
  以及`TestScatterBatchRequestAccounting`的ID批量计数/操作成员检查。
  本次这些Go测试均未执行；没有将测试名当运行证据。

实现复用真实region索引、filter复制检查、RuleManager、目标选择、operator工厂、
controller限流准入。Rules来自同一个已初始化manager，测试用真实MemoryStore支持初始化。
仅allocator I/O边界注入失败；没有mock散射或mock controller。
生成期间注入一个真实URGENT操作模拟并发准入竞争，验证随后真实Controller拒绝、
原操作保持、失败记录、processed=1、pending为空且不重试。
存储只用于初始化已有RuleManager；无新增持久化行为，不宣称持久化故障覆盖。

显式约定：升序region/store输入固定Go未指定的map/tie顺序，不声称相同随机目标。
时间仅用Tokio暂停时钟推进，错误/操作/状态不忽略。取消是任务要求的Rust增强：
Go当前循环不检查context；Rust在区域之间及退避期间检查CancellationToken，保留已完成
结果、最新错误和待处理ID，未尝试区域补Cancelled，不回滚已完成的计数或准入。
宿主负责当前hot/affinity输入、suspect(false)消费及controller心跳派发/终态归档。
本入口不扩展为server装配或internal split-scatter调度。

指标遵循add-metrics技能：保留`pd_schedule_scatter_operators_count{type,event}`，
固定11组句柄，宿主Registry注册并传播重复注册错误，不增加region/group动态标签；
`success`代表工厂创建，Controller拒绝不会改为工厂fail。未扩展distribution指标。

测试明确注册在lib.rs，4项有效测试，0忽略，共22个语义场景：

| 测试 | 案例与结果 |
| --- | --- |
| `task_061_behavior_cases` | [冻结fixture](fixtures/task_061.json)10例：空输入/单missing/全missing三个顶层Err；mixed、重复ID、范围、pending peer、负重试、hot部分失败、预取消七个Ok结果。逐一比较processed、失败ID、pending、cancelled、no-region/hot/unnecessary指标和组隔离。 |
| `task_061_retry_allocation_and_controller_rejection` | 4例：一次allocator失败后成功（skip=true/false各一）、持续失败（三次尝试）、真实Controller拒绝；2成功/2带失败，断言真实AddLearner、RemovePeer light标记、HIGH/group、类型化Allocation/Admission错误、失败映射和最终700ms退避。 |
| `task_061_cancellation_during_backoff_and_retry_cap` | 2例：50ms取消（已完成另一区保留，hot仅尝试1次）、retry=99截断为31次；两者保留准确hot错误/pending。 |
| `task_061_existing_group_and_region_faults` | 6个区域行为：首次成功、同组运行中无操作成功、其它组失败2次、epoch变化失败、无leader失败、缺replica失败；2成功/4失败。另验证重复Registry注册错误、指标名称与复制suspect(false)标记。 |

失败—通过：

- [red-api.log](logs/task_061/red-api.log)：退出101，初始缺批处理API（同时暴露测试使用
  不存在的steps getter，后修正为canonical step/len）；这不是唯一失败证据。
- [red-accounting.log](logs/task_061/red-accounting.log)：临时将processed递增改为0，
  编译成功，4项测试全部行为断言失败（actual=0/expected=1）；变异已恢复。
- [focused.log](logs/task_061/focused.log)：同组测试恢复后4通过、0失败。
- [package.log](logs/task_061/package.log)：12通过、0失败；doc-tests=0不计有效测试。

Cargo通过[run.py](logs/task_061/run.py)动态原子mkdir领取1–10共享槽位，记录任务、
计划、会话及PID；中断/失败时终止并等待子进程组后释放锁，保留缓存。
每次Rust修改后先`cargo fmt --all`。每个实际命令/退出码/槽位/绝对target在同名JSON。
`cargo generate-lockfile --offline`只增加本crate对已有依赖的引用；审查并还原其附带的
pulldown-cmark版本漂移，远程依赖版本/校验和保持原样，后续全部检查使用--locked。

最终验证全部退出0：

| 命令 | 结果 |
| --- | --- |
| `cargo test -p astersql-pd-schedule-scatter --locked --lib task_061 -- --nocapture` | 4通过，22个行为场景 |
| `cargo test -p astersql-pd-schedule-scatter --locked` | 12通过 |
| `cargo metadata --format-version 1 --no-deps` | workspace解析通过 |
| `cargo fmt --all -- --check` | 格式通过 |
| `cargo check --workspace --locked` | 检查通过 |
| `CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 1536通过、0失败、1既有忽略 |
| `git diff --check` | 通过 |

完整[workspace日志](logs/task_061/workspace-test.log)、[计数](logs/task_061/counts.json)
记录1534项非doc测试及2项compile-fail doctest；既有`tso_processes`默认忽略不计通过。
保留server/core_region_adapter.rs:134既有cpu_usage deprecated警告，无新增警告。
实际所有阶段均为**槽位1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁已释放，缓存保留，没有其它target目录。

[审计](logs/task_061/audit.json)：1101个Go/模块文件（含初始untracked清单检查）的
SHA256和空Git状态保持不变；两份只读plan.md与HEAD一致；所有远程Cargo.lock条目
版本/校验和未变。workspace格式化无范围外Rust改动，Markdown相对链接均可解析。
日志仅规范行尾空白和末尾多余空行，未改变测试断言或结果。
原有其它任务删除保持原样且不暂存；仅提交本任务实现、接线、测试、fixture和证据。
编号任务文件验证完成后按要求删除，不纳入代码提交。

# 任务064：拆分重试与取消

前置批次63由提交 `fc9eb1423` 和 [task-063](task-063.md) 核实：
canonical `SplitAttempt`、真实工厂/controller准入及扫描、包/workspace成功证据存在。
本批次仅任务64，无并行代理。实现使用现有接口，没有复制拆分或复制健康判断。

生产路径：[split_retry.rs](../../../pkg/schedule/splitter/split_retry.rs)，公开
`SplitRetry`、`SplitRetryPhase`、`SplitRetryResult`；
[领域说明与宿主驱动约定](../../../pkg/schedule/splitter/split_retry.rs.md)。
宿主调用 `start_attempt` 接入真实准入/dispatch，以显式单调毫秒时钟调用 `advance`，
结束时读取 `result`。无私有runtime、后台任务或timer；drop释放所有本地状态。

用户已解除Go运行oracle准入限制。本任务证据明确为**只读Go源码/测试契约推导 +
实际Rust行为验证**，没有Go运行输出或运行差分。
[provenance](logs/task_064/provenance.json)保存来源commit、Go文件SHA256、与计划
基准commit逐文件相同比对、fixture SHA256；binary/output hash为null。
已查阅oracle-index：既有运行案例不覆盖任务64，未改写索引。

只读来源 `region_splitter.go::SplitRegions/splitRegionsByKeys` 冻结如下行为：

- 每轮重新分组剩余keys，最多retryLimit+1轮；空输入0%，负上限不执行。
- 100ms观察节奏、每轮60s超时；只有真实start-key扫描观察算完成。
- 部分观察在timeout/cancel后保留；返回ID跨轮去重，重复keys按字节匹配消费。
- 百分比 `100 - remaining*100/original`，三个key完成一个为34%，并非33%。
- 当前已是region起点的key不准入且仍属未处理；全部边界返回0%。
- 未完成时指数退避100ms起、60s封顶，最后一轮仍等待。
- Go取消只停止当前扫描；外层继续重试及不可取消的sleep。Rust按此驱动，
  已取消上下文仍提交后续轮。取消不自动撤销controller操作。
- 空/被拒绝组仍等tick或cancel；准入错误保留 `SplitExecutionError` 类型且不计完成。
- 同时就绪的tick与取消/timeout采用取消优先，是Go select允许的确定性顺序；
  IDs按升序归一化Go map顺序。没有忽略keys、错误、比例或状态。

Go `TestMain` 是goleak/连接清理入口。`TestRegionSplitterTestSuite/TestRegionSplitter`
用mock handler验证fff/ggg的100%和范围外aaa/bbb的0%；`TestGroupKeysByRegion`
覆盖分组。它们没有取消、超时、退避上限、重复key和实际扫描运行证据。
本任务没有运行Go测试，也不将上述测试名称视作已执行证据。

[fixture](fixtures/task_064.json)保存10条固定原始输入/输出：全部完成、超时部分、
取消部分、取消未观察、超时空、重复key、整数取整、全边界、混合边界、范围洞。
`task_064_behavior_cases`逐条执行；另3项有效测试：

- `task_064_retries_remaining_and_deduplicates_ids`：真实cluster变更后重新分组，只提交
  剩余重复keys，检查真实split step keys，跨轮相同ID不重复返回。
- `task_064_cancel_retries_and_capped_final_sleep`：12轮已取消请求，首轮真实准入、
  后续真实重复准入拒绝；检查每轮退避、60s封顶及最后一次等待，绝不凭边界存在计成功。
- `task_064_empty_and_negative_limit`：两个无执行边界。

使用真实BasicCluster/RegionsInfo、ControllerAdmission、split工厂与扫描、replication
filter。RuleManager基于真实MemoryStore/StorageEndpoint创建；legacy复制分支无需初始化
规则。只控制显式时钟和外部heartbeat快照，不mock业务判定。无持久化写入或网络
副作用，不宣称覆盖存储/网络故障；故障为取消、timeout和controller拒绝。

TDD：

- [red.log](logs/task_064/red.log)：测试初次编译因CancelReason名称和AdmissionOutcome
  嵌套访问错误退出101，保留为接线修正记录，不算行为失败。
- [red-behavior.log](logs/task_064/red-behavior.log)：保留扫描比例但遗漏结果ID的实现，
  使同一fixture测试和跨轮去重测试发生实际断言失败，2通过/2失败，退出101。
  修复后同一测试通过，证明可识别错误行为。
- [focused.log](logs/task_064/focused.log)：4通过，10条fixture全部执行，无失败/忽略。
- [package.log](logs/task_064/package.log)：13通过，0失败；零doctest不计有效测试。

所有Cargo命令由 [run.py](logs/task_064/run.py) 原子mkdir动态领取仓库共享槽位1–10。
owner记录任务/计划/会话/PID；子进程继承绝对CARGO_TARGET_DIR。异常/中断先终止并等待
子进程组，finally仅释放自己的锁，不删除缓存。每次Rust编辑后先cargo fmt --all。
每条命令的确切参数、退出码及槽位记录在各日志同名JSON中。
实际所有阶段使用槽位 **1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

| 验证命令 | 结果 |
| --- | --- |
| `cargo test -p astersql-pd-schedule-splitter --locked --lib task_064 -- --nocapture` | 退出0，4通过，10条fixture |
| `cargo test -p astersql-pd-schedule-splitter --locked` | 退出0，13通过 |
| `cargo metadata --format-version 1 --no-deps` | 退出0 |
| `cargo fmt --all -- --check` | 退出0 |
| `cargo check --workspace --locked` | 退出0 |
| `CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 退出0，1549通过，0失败，1既有忽略 |
| `git diff --check` | 退出0 |

workspace只保留既有 `pkg/server/core_region_adapter.rs:134 cpu_usage` deprecated
警告，无本任务新增警告。
[审计](logs/task_064/audit.json)对照[初始清单](logs/task_064/before.json)：1101个
Go/模块文件内容、文件清单和Git状态一致；两个只读plan.md均未改动；Cargo.lock
无差异，未添加依赖。全workspace格式化没有产生范围外Rust变更。
日志仅规范行尾空白，未改写命令结果。既有其它任务删除不纳入本任务提交。

状态：已完成。

全部子进程已退出，自己的槽位锁已释放，缓存保留。workspace计数包含2项compile-fail doctest；既有忽略项不计通过。完整计数见 [counts.json](logs/task_064/counts.json)。

# 任务125：批量散射管理响应

依赖124为提交 `37b4b8a2b`，其[证据](task-124.md)记录5项聚焦、35项包级、
1807项workspace测试通过。已核验 canonical Handler/Coordinator、散射 owner、
ScatterBatch、规则和准入接口实际存在；本任务的测试真实复用这些对象。
两级 plan.md 只读，Go/模块文件严格只读。

用户后续明确授权以只读 Go 源码/测试推导与 Rust 真实行为/故障验证替代缺失的
Go 运行 oracle。oracle-index 中任务125为 source_assertion，没有冻结的本任务运行
结果；本任务未运行 Go 测试、二进制或 failpoint 工具，未声称 Go 运行差分。
[provenance](logs/task_125/provenance.json)记录来源 commit、前置提交和来源/fixture
SHA-256；Go二进制和输出为null。handler.go、region_scatterer.go、调用方
scatter_split_test.go 均与来源 `836fdc7cf3ef34f7eabc9d051418995bb97efabe` 字节一致。

生产路径为 [scatter_admin.rs](../../../pkg/schedule/handler/scatter_admin.rs)，
公开 `add_scatter_region_operator`、`add_scatter_regions_operators`、
`scatter_regions_by_range`、`scatter_regions_by_id`、`build_scatter_regions_resp`。
既有输入分派改为委托这些入口，保持其状态/JSON映射；没有复制 canonical scatter。
[领域说明](../../../pkg/schedule/handler/scatter_admin.rs.md)解释可变 Coordinator
借用、计数、边界与错误。单区按 Go addOperator 直接准入并确认/归档替换项；批量
复用已有真实 RegionScatterer，结果保留 processed、失败map、pending、suspect和取消。

[fixture](fixtures/task_125.json)冻结7行比例预期，来自只读公式而非待测实现。
其它独立字面预期见[测试](../../../pkg/schedule/handler/task_125_test.rs)。
没有忽略ID、错误、操作或状态；仅按BTreeMap/BTreeSet排序观察ID集合，无其它归一化。

| 测试 | 有效表驱动案例及观察 |
| --- | --- |
| task_125_behavior_cases | 7行比例：0/0、全成功、全失败、1/1、2/1、1/2、31/1；精确取整 |
| task_125_batch_selection_partial_and_retry | 6行：重复missing ID失败去重、范围优先、单边非法hex被忽略、重复ID去重、负retry无尝试、hot部分失败与重试 |
| task_125_errors_single_and_bootstrap | 2行单区hot/不存在、6行批量空输入/单missing/全missing/两端hex错误/空范围、3种bootstrap与解析顺序 |
| task_125_allocation_faults_and_single_admission | 4行：分配I/O首次失败再成功、永久失败、单区成功与单区故障；真实步骤目标store4/peer900、epoch、HIGH、非lightweight删除、同组重复请求原Arc保留；永久失败700ms背退且无操作入队 |
| task_125_explicit_range_retry_cap_and_cancellation | 3行retry i64最小值/31/最大值；显式空键全范围、3个pending、上限重试及取消保留missing和Cancelled错误 |

共31个顶层表驱动/独立错误案例（7+6+11+4+3），另含每行内部的状态/故障断言。
测试使用真实默认规则初始化及MemoryStore/StorageEndpoint，不模拟内部调度组件。
仅暂停宿主Tokio测试时钟、控制hot快照和allocator I/O。管理动作本身没有持久化写入，
不编造存储故障。TTL队列逐一await shutdown。单区suspect转发另由既有task121包测试覆盖。

[initial](logs/task_125/initial.json)记录缺失API exit101，仅作初始接线红灯。
[implemented](logs/task_125/implemented.json)暴露装配缺默认规则与无界尾region扫描；
[corrected](logs/task_125/corrected.json)修正规则初始化和空扫描范围后3项通过。
[expanded](logs/task_125/expanded.json)暴露5个store会产生两个peer分配，不能断言一次
成功分配；改为4个store固定一个迁移目标后[faults](logs/task_125/faults.json)5项通过。
这些测试装配失败不冒充生产行为红灯。

[behavior-red](logs/task_125/behavior-red.json)保留同一完整测试对错误实现的实际检测：
把公式改为 `100*processed/总数`、范围条件改为OR，exit101，3通过/2失败；具体断言
66不等于67、单边非法hex被错误解析。随后恢复正确代码并重新格式化和验证。

最终验证结果见 [final.json](logs/task_125/final.json)，原始stdout/stderr按gzip保存，
包括确切命令、退出码、日志哈希与test result。所有Cargo命令由
[verify.py](logs/task_125/verify.py)原子mkdir动态领取共享槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话/PID；正常、异常和中断先终止并等待自己的子进程，然后释放锁，
缓存保留。Cargo及子进程继承该绝对路径；CARGO_BUILD_JOBS=4、RUST_TEST_THREADS=4、
GOFLAGS=-mod=readonly。没有新增依赖/feature或Cargo.lock变化。

| 最终命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化完成 |
| `cargo test -p astersql-pd-schedule-handler --locked --lib task_125 -- --nocapture` | 0 | 5通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-handler --locked` | 0 | 40通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace装配通过 |
| `cargo fmt --all -- --check` | 0 | 格式检查通过 |
| `cargo check --workspace --locked` | 0 | workspace检查通过 |
| `cargo test --workspace --locked` | 0 | 1812通过、0失败、1既有忽略 |

workspace数量含2项compile-fail文档测试；没有新增忽略。仅出现既有server deprecated
字段warning。[最终只读复核](logs/task_125/verification.json)由
[check_readonly.py](logs/task_125/check_readonly.py)对比起始压缩清单：1101份Go/模块
文件（含未跟踪路径）与两份只读plan.md的路径及内容哈希均不变，Go/模块Git状态为空。
`git diff --check`通过，新增文档相对链接已核实。自己的槽位锁已释放，缓存保留。
仅本任务实现、测试、局部接线、说明及验证证据纳入提交，其他任务已有删除不纳入。
状态：已完成。

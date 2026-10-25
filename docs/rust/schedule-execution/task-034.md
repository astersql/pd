# 任务034：操作准入、替换与移除

生产路径 [controller_admission.rs](../../../pkg/schedule/operator/controller_admission.rs)
公开 `ControllerAdmission`、`AdmissionOutcome`、`AdmissionError`。复用真实
BasicCluster、Operator、RandBuckets 和 WaitingOperatorStatus；领域边界与宿主
组合方式见 [controller_admission.rs.md](../../../pkg/schedule/operator/controller_admission.rs.md)。
仅注册本任务实现与独立测试，未修改 Cargo.toml/Cargo.lock 或其它生产模块。

## 依赖与来源

批次33提交 `2de834b22` 的 [task-033](task-033.md) 记录队列5个测试及1420个
workspace测试通过。已核实 canonical Operator 的 start/cancel/replace、过期、
优先级、additional-info API，以及队列的配对取出与独立计数，满足实际前置依赖。

[fixture](fixtures/task_034.json) 冻结基准
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 的 controller 源码与测试 SHA256、
11组准入输入/预期，以及配对、身份、队列、过期与并发场景。两份 Go 文件均与
基准逐字节一致。证据类型 `source_assertion`、`go_execution=false`。遵照用户
明确授权，以只读 Go 源码与测试契约推导预期；未运行 Go，不宣称运行差分。

已读取 oracle-index.json。既有 `operator_concurrent_replace` 仅覆盖旧引用
移除与并发替换，不能代表本任务完整准入/配对/队列覆盖。本任务没有把该记录
冒充新 Go 执行证据。只读对照入口及其实际行为：

- `TestCheckAddUnexpectedStatus`：非 CREATED 拒绝、全部配对成员过期检查。
- `TestConcurrentRemoveOperator`：旧 dispatch 引用不得移除新操作。
- `TestConcurrentMergeConflict`：配对冲突后实际 map 与主 kind 计数一致。
- `TestAddWaitingOperator`：等待描述配额、配对入队和推进。
- `TestMergeOperatorsSynchronousCancellation`：merge/affinity 元数据驱动的成对
  取消；不以是否含 OpMerge 位来判断关联。
- `TestConcurrentAddOperatorAndSetStoreLimit`：并发新增与 store-limit 更新；
  本任务只实现准入容器，store-limit 及 Dispatch 留给后续批次，不声称覆盖该
  Go 测试全部行为。

Go 当前使用 sync.Map 分步提交；本任务要求的原子配对在 Rust 用同一容器锁实现，
批量检查、替换旧成员、移除残余旧 partner，再插入新成员。该原子保证及重复/
孤立/非互反配对的类型化错误有独立 Rust 断言，不描述为 Go 分步并发行为的
逐指令差分。时间仅采用显式 monotonic clock 检查3秒边界；随机值由宿主输入，
不忽略错误、状态或操作成员。没有本任务持久化或外部 I/O 路径。

## 测试与失败证据

[task_034_test.rs](../../../pkg/schedule/operator/task_034_test.rs) 显式注册行为
测试，覆盖 region 缺失、version/confver 不等、nil epoch、同/低优先级拒绝、
高优先级替换、非 CREATED 拒绝、等待配额、身份移除、配对替换/移除/拒绝、
waiting promote 跳过 stale、全部成员过期、非法随机/优先级及800次并发配对请求。
主 kind 计数按精确键验证，拒绝和移除均核对真实容器状态。额外直接消费
RangeOperatorFactory 的 MERGE/AFFINITY 两种真实配对产物，验证 active/passive
步骤、一次等待计数、成对启动与取消，以及 AFFINITY 不计入 MERGE。

[missing-api.out](logs/task_034/missing-api.out) 保存首次缺失 API 的编译失败
（退出101，同时修正测试 epoch 字段类型），不作为行为失败。
[red.out](logs/task_034/red.out) 保存错误变异“允许相同优先级替换”触发同一个
`task_034_behavior_cases` 的拒绝断言失败，7通过、1失败，退出101。恢复正确比较器后执行格式化
和全量验证。变异脚本会在 finally 恢复代码；错误实现没有保留。

实际注册的8个测试：

| 测试 | 有效案例 |
| --- | --- |
| `task_034_behavior_cases` | fixture 11组，11成功、0失败 |
| `task_034_replace_and_identity_remove` | 替换终态、旧引用移除、精确kind计数 |
| `task_034_merge_atomicity` | 配对拒绝、交叉替换、同区域配对替换与成对移除 |
| `task_034_waiting_promote` | 配额、promote绕过配额、缺region丢弃、配对与NaN |
| `task_034_expiry_and_invalid_batch` | 单项过期、重复/孤立/空批次拒绝、nil与零epoch等价 |
| `task_034_concurrent_admission_remove` | 8线程×100轮真实配对请求，线程全部join |
| `task_034_retry_expired_pairs_and_queue_errors` | stale后继续推进、两成员3秒过期、priority4与零配额 |
| `task_034_factory_merge_and_affinity` | 工厂MERGE/AFFINITY两类产物入队/启动/移除 |

## 执行与只读边界

所有 Cargo 命令经 [slot-run.py](logs/task_034/slot-run.py) 原子领取共享槽位。
本任务实际槽位1，绝对路径：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁记录计划、任务、会话与 PID，子进程退出后释放自有锁并保留缓存。

[readonly-validation.py](logs/task_034/readonly-validation.py) 复用先前已存在的
etcd外部 I/O fixture二进制；[Go拦截器](logs/task_034/reuse-go/go) 拒绝所有真实
Go命令，只允许校验哈希后复制现有 fixture。其
[provenance](logs/task_034/fixture-reuse-provenance.json) 与
[复制事件](logs/task_034/go-reuse.jsonl) 保留实际来源；这不是本任务 Go oracle。
[validation.json](logs/task_034/validation.json) 保存确切 Cargo 命令、退出码、
有效测试数、绝对目标路径；各命令完整 stdout/stderr 位于相邻 validation-N.out。

[verification.json](logs/task_034/verification.json) 由
[verify.py](logs/task_034/verify.py) 保存所有1101份 Go/模块文件哈希前后相同、
Go Git 状态为空、两级只读 plan.md 未变、fixture哈希、来源匹配与链接检查结果。
未运行 make、failpoint、Go格式化或生成入口。其它任务已有删除排除在本次提交外。

## 最终结果

状态：已完成。全部必需命令退出0：

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_034 -- --nocapture` | 0 | 8通过，0失败，0忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 65通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1428通过，0失败，1忽略 |

聚焦测试8通过，fixture固定场景11成功/0失败，另有800轮并发配对请求及两类
真实工厂产物。workspace的1项忽略来自既有测试。正确实现与最终fixture均已
纳入最终全量运行；没有零测试替代行为验证。全部验证子进程已退出，自有槽位1
锁已释放，缓存保留。`git diff --check` 和 Markdown 相对链接检查通过。使用
git-commit 技能只提交本任务相关代码、测试与证据；编号任务文件按授权删除，
不将其它任务已有删除纳入提交，不修改两级只读 plan.md。

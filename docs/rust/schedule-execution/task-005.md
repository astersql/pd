# 任务 005：过滤结果与基础候选筛选

批次 5，依赖批次 4 提交 `9f430953c`。已核实[任务 004](task-004.md)的完整
运行证据、canonical core/config/plan API 和准入 `oracle-index.json`；过滤行为
在准入时没有冻结输出，本任务先补齐 Go 运行结果，再实现 Rust。

## 实现边界

新增 canonical crate `astersql-pd-schedule-filter`，只包含 Filter 契约、
ExcludedFilter、SelectSourceStores/SelectTargetStores/SelectUnavailableTargetStores
对应的三个 snake_case 函数；[生产边界说明](../../../pkg/schedule/filter/selection.rs.md)
记录对象身份、返回 Vec、配置借用、错误和后续计数器边界。

使用真实 core StoreInfo、配置 ProviderSnapshot、plan Status 和 Collector，没有
复制状态面、mock 返回状态、I/O 或新 feature。source/target 集合分开；保留候选
顺序、重复项和 Arc 身份。每个 store 按顺序短路，collector 只保存首个拒绝的完整
Status。普通筛选沿用 base step，unavailable target 强制 step 2，base 不变。
Counter/Prometheus 生命周期属于任务 11，StoreStateFilter 属于任务 7。

## Go oracle 与冻结输入

[fixture](fixtures/task_005.json)、[原始运行输出](logs/task_005/oracle.json)、
[调试命令](logs/task_005/oracle.commands)、[来源清单](logs/task_005/sources.json)
和[provenance](logs/task_005/provenance.json)可逐项审查。
[normalize.py](logs/task_005/normalize.py)仅从 Go transcript 提取结果，未读取 Rust。

来源固定 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`。`filter.test` 是只读 Go
交叉编译产物；`schedulers.test` 和 dlv 复用批次 3/4 并校验 SHA256。
[编译记录](logs/task_005/compilation.json)保存实际 argv/env/退出码；编译只写
外部二进制/cache，Go 模块设为 readonly，未调用 failpoint 工具或生成器。

逐项核查所列五个过滤测试：DistinctScore、LabelConstraints、RuleFit、
RuleFitWithPlacementRule 验证各自的具体过滤器，SendState 验证 region snapshot
选择；它们不直接覆盖本任务 collector 步骤。另有 TestCandidates 实际调用 source
和 target selectors。[safe-tests.json](logs/task_005/safe-tests.json)保存这六项
已有 Go 测试的完整 PASS，命令退出 0；没有把它们冒称完整 selection oracle。
所选测试、setup/helpers/TestMain 的运行不需要 failpoint 注入，没有裸跑整个 Go 包。

完整 selection 输出来自既有 TestBalanceLimit 的真实 dry-run frame，调用真实
excluded 构造器、三个 selector 和 collector。SnapshotSenderFilter 构造器仅用于
建立同类型的真实 ID map；调试器 `-unsafe` 允许同步调用传递 frame 内已有 slice。
调试器设定输入 NodeState=3，并保留原 StoreStateFilter 作为后续拒绝对照；没有
修改算法、返回值或断言。调试结束前退出宿主测试，因此这些是实际 API 观察，
不宣称修改输入后的完整 TestBalanceLimit PASS。

| 冻结案例 | 观察 |
| --- | --- |
| source_empty / target_empty | 保留全部四个候选，collector 无条目 |
| unavailable_empty | 无候选，collector 无条目 |
| excluded_source | 拒 source 集合的 ID，step=0/status=101 |
| excluded_target | 拒 target 集合的 ID，step=0/status=101 |
| excluded_unavailable | 选择被拒 target，step=2/status=101 |
| target_step2 | 普通 target 继承 step=2 |
| first_excluded | excluded 在 removed store 上先返回101，后续 state 拒绝不可达 |
| first_state（上下文） | Go state filter 首先返回401，留给后续迁移，不计入 Rust 差分 |

前八项为本任务的可观察执行前缀。first_excluded 在 Rust 中的第二项也使用真实
excluded filter，首项返回后它不可达；独立测试检查首拒绝索引和 scope，倒序错误
变更会失败。没有为实现范围外的 state filter 编写固定返回值替身。

归一化仅将 StoreInfo 地址按同次运行的 GetID 映射为 ID，移除调试地址/日志时间；
保留输入及输出顺序、空结果、step、status、reason 和资源 ID。没有排序后隐藏
筛选次序，也没有从 Rust 反推期望。

## 失败、边界与故障证据

[red-compile.out](logs/task_005/red-compile.out)记录最初缺失 selection 模块/API，
聚焦命令退出101，零行为测试，仅作初始缺失证据。
实现后[mutation.py](logs/task_005/mutation.py)使用同一聚焦命令实际检出四类
错误；finally 恢复原实现并格式化。[mutation.json](logs/task_005/mutation.json)
保存 argv、退出码和有效数量，均为断言失败，没有把编译错误算作行为失败。

| 错误变更 | 当时有效结果 |
| --- | --- |
| source 使用 target 集合 | 1通过/1失败，退出101 |
| 逆序查找拒绝过滤器 | 1通过/1失败，退出101 |
| unavailable 不设置 step2 | 1通过/1失败，退出101 |
| 反转选中条件 | 0通过/2失败，退出101 |

`mutation-setup-attempt.out` 保留首次脚本匹配格式错误；恢复源码后修复脚本，以上
四次为最终完整执行。初次 Cargo 离线生成锁文件带来的无关 pulldown-cmark 升级
已恢复，最终只新增本地包及其依赖。

最终 Rust 三项测试：task_005_behavior_cases（8个冻结前缀案例），
task_005_order_and_boundaries（source/target 首拒绝索引、首项通过后继续、完整u64
ID、零ID、重复候选、空输入、无collector、真实Arc身份），
task_005_collector_resource_fault（真实collector错误资源类型与后续可用性）。

[fault.json](logs/task_005/fault.json)与[fault.commands](logs/task_005/fault.commands)
记录 base step=1 接收 Store 时 Go 实际 runtime.TypeAssertionError；Rust 捕获
canonical plan 的对应类型不匹配 panic，并检查未追加计划、base 不变；之后
unavailable selection 强制 step2 可以正常追加。Go调试器未显示 Error() 文本，
最初捕获脚本的文本检查过严，最终审计按实际 TypeAssertionError 核验，不声称
跨语言 panic 字符串一致。没有持久化或外部I/O，故无需构造MetaStore故障。

## 最终验证

状态：已完成。fixture SHA256：`7b8a2c8a020796c73b3af10eb3560e80ed92e5f767993891895b7293b64d56d0`。

[validation.json](logs/task_005/validation.json)保存精确 argv、退出码和有效数量，
对应 `validation-0.out` 至 `validation-6.out` 保存输出。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式/接线/编译检查 |
| `cargo test -p astersql-pd-schedule-filter --locked --lib task_005 -- --nocapture` | 0 | 3通过/0失败/0忽略 |
| `cargo test -p astersql-pd-schedule-filter --locked` | 0 | 3通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 格式/接线/编译检查 |
| `cargo fmt --all -- --check` | 0 | 格式/接线/编译检查 |
| `cargo check --workspace --locked` | 0 | 格式/接线/编译检查 |
| `cargo test --workspace --locked` | 0 | 1329通过/0失败/1忽略 |

既有忽略为 `tso_processes`，依赖独立进程兼容脚本；没有新增忽略。既有 proto
弃用字段警告不影响退出码。展示日志只移除 ANSI 和行尾空白，Go 原始 stdout/stderr
始终保留在 JSON 中。

所有 Cargo 阶段均由 [slot-run.py](logs/task_005/slot-run.py)动态原子领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁记录 plan/task/session/PID；Cargo 及其子进程均继承同一绝对目录；正常、失败和
INT/TERM 路径等待自身子进程退出后清理自有锁，不清理缓存或其他任务的锁。
最终验证全部退出0，自有锁已经释放，缓存保留。

[verify.py](logs/task_005/verify.py)核对起止全部 Go/模块文件 SHA256 与 Git 状态，
包括 ignored/untracked；核对17份只读 plan.md，全部一致。来源、fixture原始输出、
复用二进制、完整Go测试、行为失败记录、最终测试计数、依赖方向和仅本地锁文件
变更均通过审计，结果在[证据审计](logs/task_005/evidence-verification.json)和
[Go审计](logs/task_005/go-verification.json)。`git diff --check` 及暂存差异检查
通过，仅提交本任务源码、测试、接线与证据，并删除已完成的编号任务文件。

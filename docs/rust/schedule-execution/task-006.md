# 任务 006：候选排序与选择

批次 6，前置批次 5 提交 `82e885286`。已审阅[任务 005](task-005.md)的运行
差分、3 项包测试及 1329 项 workspace 测试证据；本任务复用真实 selection、core、
配置和 plan API。准入 oracle-index 尚未冻结候选比较器，本任务另行补齐。

## 实现

[生产实现](../../../pkg/schedule/filter/candidates.rs)导出 StoreCandidates、
region_score_comparer、isolation_comparer 和 RandomIndexError；
[边界说明](../../../pkg/schedule/filter/candidates.rs.md)说明借用、次序和随机输入
契约。只接入 filter/lib.rs，没有新增依赖、Cargo.lock 变更、feature、状态副本或
server/runtime 依赖。KeepTop 的交换次序、PickTop 的首项并列规则均保留。

## Go 运行 oracle

[fixture](fixtures/task_006.json)、[来源哈希](logs/task_006/sources.json)、
[provenance](logs/task_006/provenance.json)固定 source commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`，当前 Go/模块与该基准无差异。
filter.test 与 dlv 复用任务 005，SHA256 校验一致。现有 filter/schedulers 二进制
未保留两个比较器；已通过现有 checker 的实际调用链补齐：只读编译现有 Go 测试，
输出写 /tmp 二进制和外部 cache，GOFLAGS=-mod=readonly，无源码改写、生成器、
failpoint 注入或模块整理。精确编译 argv/env/退出0保存在 provenance。

TestMain 是 goleak + WaitForEtcdConnections 包装，不能算成候选行为测试。
TestCandidates 覆盖 source/target 链、空 First/Random、min/max、Sort、Shuffle、
RandomPick 与 asc/desc KeepTop，但没有调用两个生产比较器。
检查 checker TestBasic、TestDistinctScore 的 setup/teardown 与相关 mockcluster、
strategy 路径后，运行这些现有测试无需 failpoint 注入；[完整输出](logs/task_006/safe-tests.json)
记录 filter 1 项、checker 2 项行为测试均 PASS、退出0，父 suite 不重复计数。

[comparers.commands](logs/task_006/comparers.commands)在真实 comparer 返回分支
设置 tracepoint，未修改算法、返回值或断言；[原始输出](logs/task_006/comparers.json)
保留实际执行的返回分支、sa/sb、两侧 store 的 ID、labels、regionSize、weight、
capacity/available/used。Isolation 仅取 regionStores 长度2的执行，保留两个参考
store 的 ID 和完整 labels。真实配置 getter 观察为 v2、high=0.7、low=0.8。
冻结去重后共23例：13 RegionScore（正/负），10 Isolation（正/负/并列），另有
[tie](logs/task_006/tie.json)在评分开始前只设置领域输入 b=a，stepout 实际返回0，
随后退出；这是独立 API 观察，不宣称改变输入后的完整测试 PASS。

[candidates.commands](logs/task_006/candidates.commands)与[输出](logs/task_006/candidates.json)
记录 TestCandidates 实际检查点：空集合、min/max、完整排序、随机重排、随机选择、
顶部并列 asc/desc。Go math/rand/v2 的 Shuffle swap 前记录6个实际 (i,j) 样本，
Rust 在同一 bounded-random 输入边界重放，比较全部有序结果；RandomPick 对实际
sorted 输入记录选中 ID 和其索引。没有通过排序输出隐藏 shuffle 差异。

[normalize.py](logs/task_006/normalize.py)只读 Go transcripts，保留全部领域数值、
次序和评分，不读取 Rust；归一化仅去掉日志时间与内存地址，指针身份在 Rust 中
另用 Arc::ptr_eq 验证。13+10例均逐一比较实际输出，浮点评分只容许1e-12相对误差，
比较结果必须完全一致。不同 RNG 的 seed/原始位流不属于差分契约。

## 失败与边界证据

[red-compile.out](logs/task_006/red-compile.out)：新增并注册同一组三项测试后，缺少
StoreCandidates/比较器 API，退出101，零行为测试；仅作初始缺失证据。
[green-initial.out](logs/task_006/green-initial.out)保留首次行为运行发现 fixture 提取
错误的失败：空 `[]string` 类型前缀被误认成 labels 数据，导致 Isolation 断言失败；
修复 normalize.py 的字段定位后，fixture 从同一原始 Go 输出重新提取。

[mutation.py](logs/task_006/mutation.py)随后运行同一聚焦测试，证明真实行为检测能力；
每次修改后 cargo fmt，finally 恢复原实现。[结果](logs/task_006/mutation.json)：

| 错误实现 | 通过/失败 | 退出码 |
| --- | --- | ---: |
| 反转 RegionScore 比较 | 2/1 | 101 |
| 反转 Isolation 比较 | 2/1 | 101 |
| 丢弃顶部并列项 | 1/2 | 101 |
| shuffle 不交换 | 2/1 | 101 |

最终3项测试：task_006_behavior_cases（23个比较案例+1个同对象并列+5个确定候选
检查：min/max/sort/asc/desc），task_006_random_replay_and_faults（完整6步shuffle、
random pick、Arc身份、非法pick索引、首个非法shuffle索引、后续非法shuffle索引
及已交换前缀），task_006_boundaries_and_filter_chain（空/单项、u64::MAX、首并列、
不重排、unsorted KeepTop、真实ExcludedFilter source/target链和身份）。
Rust额外边界按来源契约验证，未冒称都拥有独立 Go 运行输出。随机错误是新增宿主
输入边界的契约违规检测；Go内部RNG不会产生该输入。不涉及外部I/O/MetaStore，
无需构造无关存储故障。

## 最终验证

状态：已完成。fixture SHA256：`302c6f592a8d77ef333c6510dc3c831d34339daae9a6b8bc45fbf61e50abe0b6`。

[validation.json](logs/task_006/validation.json)保存精确 argv、退出码、有效测试数量，
`validation-0.out` 至 `validation-6.out` 保存对应输出。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式检查 |
| `cargo test -p astersql-pd-schedule-filter --locked --lib task_006 -- --nocapture` | 0 | 3通过/0失败/0忽略 |
| `cargo test -p astersql-pd-schedule-filter --locked` | 0 | 6通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据检查 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace编译检查 |
| `cargo test --workspace --locked` | 0 | 1332通过/0失败/1忽略 |

既有忽略项为 `tso_processes`，没有新增忽略。proto既有弃用字段警告未导致失败。
[slot-run.py](logs/task_006/slot-run.py)各阶段原子动态领取槽位1，使用绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
记录plan/task/session/PID，所有Cargo子进程继承环境；正常/失败/INT/TERM均等待自身
子进程退出后释放自有锁，缓存保留。所有阶段结束，自有锁已释放。

[verify.py](logs/task_006/verify.py)核对Go/模块全部文件起止哈希与Git状态（包括
untracked/ignored）、17份只读plan.md、冻结原始输出哈希、来源、二进制、实际测试
数量、行为变异失败、随机tape重放一致性及生产依赖方向。见[Go审计](logs/task_006/go-verification.json)
和[证据审计](logs/task_006/evidence-verification.json)。Cargo配置/锁文件未变，
`git diff --check`及暂存差异检查通过。本任务仅提交候选实现、接线、测试和证据，
按执行约定删除完成的编号任务文件；两级plan.md与原主题均保持只读。

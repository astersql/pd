# 任务044：规则异常角色修复

状态：已完成。所有必需验证退出0，33个场景通过；Go契约为源码推导。

生产入口是 `pkg/schedule/checker/rule_fault.rs` 的
`RuleFaultChecker::check_with_fit`，并提供 `allow_leader`、
`record_region_promote_to_non_witness`、`offline_leader_count`、`refresh`。
canonical crate 为 `astersql-pd-schedule-checker`；lib.rs 显式注册模块及独立测试。
任务43的 `RuleMissingChecker::add` 仅放宽为 crate 可见，以便按单条 rule 的原始
顺序复用完整 fit，不复制补缺逻辑。未增加依赖、feature 或 Cargo.lock 变更。

## 依赖与契约来源

批次43真实依赖为提交 `9eff3f3f2`（schedule/checker: port missing placement rule peers），
[任务43证据](task-043.md) 保存26场景、包级10测试和 workspace 1468测试的通过记录。
本任务已读取 oracle-index.json；用户最新授权解除 Go 运行 oracle/差分准入限制。
本任务完全使用**只读 Go 源码/测试契约推导 + Rust 运行验证**，未执行 Go，不宣称
取得 Go 原生运行输出或运行差分。[provenance](logs/task_044/source-provenance.json)
记录源码 commit、逐文件 SHA256 与冻结 fixture SHA256。

只读来源：rule_checker.go 的 fixRulePeer、replaceUnexpectedRulePeer、
fixLooseMatchPeer、allowLeader、isDownPeer、hasAvailableWitness、recorder.refresh；
相邻 TestFixRole、TestFixRoleLeader、TestFixRuleWitness2/3/4/6、TestDisableWitness、
TestFixDownPeer、TestFixDownPeerWithAvailableWitness、TestFixOfflinePeer、
TestPreferAddTiFlashLearnerOverOfflinePeer 与
TestRecorderRefreshLooksUpStoreIDNotCount。operator/create_operator.go、builder.go、
kind.go 冻结步骤顺序、保留原 peer ID 的转换、kind 与优先级。
上述测试名称仅是契约定位，均不算 Go 已运行证据。

## 行为和故障

[fixture](fixtures/task_044.json) 保存29个固定案例，涵盖：learner 晋升、leader/follower
修复与拒绝、voter 降级、voter/learner witness 与 non-witness 转换、能力关闭、pending
和最近晋升抑制、down 时长阈值（不以 DownSeconds 替代 store 心跳时长）、短暂 down
晋升可用 witness、不可用 witness、offline、TiKV fast failover、永久/临时候选拒绝、
allocator 失败、暂停、无 leader 和缺失 store 后的真实补缺。

另外3个独立测试覆盖4个场景：

- recorder/leader replacement：原 leader 参与严格最小值平局；下一次选择健康 follower，
  transfer 在添加之前；13次真实构建分配计数得到(5,4,4)，allocator 失败不计数；
  五分钟边界不清理，超过边界按缺失 store ID 清理（计数值4/5对应 live store）。
- cooldown/allow leader：到期前1ns与到期时分别抑制/允许转换；拒绝 learner、witness、
  缺失 store；真实标签与 StoreStateFilter 被复用。
- TiFlash down：learner 替换保持 non-witness/HIGH，完整 AddLearner/RemovePeer 顺序。
- offline TiFlash 预检：首条 offline voter rule 优先补缺 learner，操作先转移健康 leader
  再添加 TiFlash learner，无 retry 意图。

共4个 Rust 测试、33个行为场景，全部0失败/忽略。每个成功操作比较有序步骤、描述、
kind、优先级、region ID/epoch；真实 RegionInfo 心跳逐步应用操作直至 SUCCESS，
原输入不变。扩展场景检查真实状态/命令结果。只有时钟与 allocator 外部边界受控。
本阶段消费已有内存 snapshot，没有新增存储写入，不虚构持久化故障点。
孤儿清理和位置优化属于后继阶段，不在此 API 内实现。

## 红灯与错误检测

[red-api.log](logs/task_044/red-api.log) 保存实现前 module/API 缺失的 exit101。
green-first 至 green-7 及 green-expanded 日志保留真实开发过程：修正 RegionInfo 查询
API 名称，依据 Go builder.go/kind.go 修正 fixture 的 kind 数值与保留 peer ID 的
non-witness 四步骤；明确 rest rule 为 voter，leader-witness 防御案例采用显式旧 fit
（fit_leader=2，应用时 leader=1），因为实时 witness fit 本身不会把 leader 配给 witness。
TiFlash 预检期望包含 Go builder 的 before-add leader transfer。没有修改生产行为来
迁就错误期望，也不把这些输入修正当作错误行为识别证据。

[mutation.py](logs/task_044/mutation.py) 对正确生产源注入六种错误：down阈值由>=改为>、
关闭 fast failover、跳过 learner 晋升、反转冷却判断、recorder.keys 改为 values、
URGENT 降为 HIGH。[mutations.json](logs/task_044/mutations.json) 保存六项均exit101，
测试内具体行为断言失败，未以编译失败代替。首轮脚本原要求日志含“assertion”，
但 Rust assert!(..., message) 只输出自定义消息，故修正 harness 为检查测试文件内 panic
并排除编译错误；生产源由 finally 恢复，再运行完整基线。

## 验证资源

[run.py](logs/task_044/run.py) 每阶段依次原子 mkdir 动态领取仓库共享槽位1–10，
记录 task/session/PID。正常、失败、信号中断均等待子进程退出再释放自己的锁，保留缓存。
实际全部阶段领取槽位1，绝对路径：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[validate.py](logs/task_044/validate.py) 与 [commands.json](logs/task_044/commands.json)
记录确切命令、退出码及有效测试计数。workspace 回归设置 RUST_TEST_THREADS=2。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_044 -- --nocapture` | 0 | 4通过，0失败，0忽略 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 14通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1472通过，0失败，1忽略 |

## 只读和交付审计

[before.json](logs/task_044/before.json) 保存任务开始时 Go/模块文件及两个只读 plan.md
逐文件哈希和 Git 状态；[audit.json](logs/task_044/audit.json) 保存最终比较、fixture哈希、
新增 Go 文件检查。未执行 Go/make/failpoint，不修改整体/包级计划或原主题。
既有编号任务删除排除在本次提交之外。日志只清理行末空白和多余末尾空行，不改语义。

最终 workspace 1472通过、0失败、1既有忽略（独立进程测试 `tso_processes`）。
实际槽位1锁已释放，编译缓存保留。

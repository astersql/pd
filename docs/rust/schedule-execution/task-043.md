# 任务043：规则缺失副本修复

状态：已完成。

实现：[rule_missing.rs](../../../pkg/schedule/checker/rule_missing.rs)，
[领域说明](../../../pkg/schedule/checker/rule_missing.rs.md)，
[独立测试](../../../pkg/schedule/checker/task_043_test.rs)。API 为
`RuleMissingChecker::check/check_with_fit/fix_missing_tiflash_learner_peer`，
以及 `RuleMissingContext/Outcome/Error` 和 `is_tiflash_learner_rule`。

## 真实依赖与来源

批次42提交 `d40dabb0c`、[task-042](task-042.md) 的实现与已完成验证证据均已核实；
复用 PauseController、ReplicaStrategy、ReplicaFaultContext、PeerOperatorFactory
和 canonical placement fitting/cache。新增3项 dev-dependency 仅用于真实
RuleManager + StorageEndpoint + MemoryStore 测试装配；Cargo.lock 仅增加这3个
workspace 路径依赖，无远程版本变更。

已审阅 oracle-index.json。依据用户后续授权，使用只读 Go 源码/测试推导期望，
**没有运行 Go，没有 Go/Rust 运行差分证据**。来源 commit、4个 Go 源文件 SHA256、
固定 inputs/expected outputs 见 [fixture](fixtures/task_043.json)。
来源是 rule_checker.go 的 Check/CheckWithFit、addRulePeerWithOptions、
fixMissingTiFlashLearnerPeer、isTiFlashLearnerRule、strategy/handleFilterState，
operator/create_operator.go 与 builder.go 的真实步骤和 kind 约定。
相邻 TestAddRulePeer、TestAddRulePeerWithIsolationLevel、TestRuleCache、
TestPreferAddTiFlashLearnerOverOfflinePeer、
TestPreferAvailableTiFlashLearnerRuleOverOfflinePeer、
TestDoNotPreferTiFlashSwapFitOverOfflinePeer 和 placement 场景表提供行为契约。
`TestRecorderRefreshLooksUpStoreIDNotCount` 已只读核实：它检查 offline leader
计数器按 store ID 清理，属于异常替换/recorder 阶段，本任务无 recorder，未把其名称
算作本任务覆盖或运行证据。

## 行为与故障证据

`task_043_behavior_cases` 22个固定案例：voter、learner、follower、leader 补齐，
Check/CheckWithFit 入口，witness capability 生效/关闭，TiFlash 普通/预检添加，
store 不足、busy 临时阻塞、暂停、无 leader、nil fit、已满足、分配器失败、
isolation/标签拒绝、TiFlash 预检不入队、不误认 compute/voter、空规则。
每个成功操作比较 desc/kind/HIGH/region/epoch 和全部有序步骤（含 witness），
逐次应用真实 RegionInfo 心跳推进到 SUCCESS；原始快照不变。

`task_043_cache_invalidation` 验证候选缓存、真实 promoted cache 指针命中、
CheckWithFit 接受缓存 fit、规则更新失效以及更新后真实补副本。
`task_043_multiple_rules_and_swap` 的3个场景分别验证：TiFlash 跳过无目标规则后
添加可用 learner（无 retry 意图）、普通检查保留前一规则 NoStoreToAdd 但成功时
清除 pending、swap-fit 将已满足的 broad learner 从受约束 store2 搬至 store3，
发出完整 AddLearner/RemovePeer 顺序。最后一个使用真实 fit_rule 组成显式
CheckWithFit assignment，固定 broad learner 已占有 store2 的有效输入，
不依赖全局拟合等价解的挑选次序。共26个有效场景（22+1+3），全部通过。

仅时钟10000秒、peer ID分配1000与外部 allocator 失败受控制；store load 按 ID
确定，避免等分随机挑选。没有忽略任何输出字段。真实 MemoryStore 用于测试
规则管理器初始化/变更，checker 本身无持久化 I/O，不虚构 storage 故障注入点。

## 红灯与错误行为识别

[red-api.log](logs/task_043/red-api.log) 保存实现前缺失 module/API 的 exit101。
初次 locked 检查要求补齐路径依赖，离线更新锁后保留红灯；审查并恢复无关的
pulldown-cmark 版本漂移，最终锁仅增加测试依赖。
green-first/green-2/green-3/green-4.log 如实保留测试输入/API修正过程：
exclusive engine 标签必须配套规则约束、显式 Serving 状态、cache场景使用无
engine 标签 store、StoreInfo 使用 set_labels API。早期 checksum 手动恢复笔误
已从 HEAD 精确恢复；不是当前环境故障，也不用于降低验证标准。

[mutation.py](logs/task_043/mutation.py) 在真实生产源引入5种错误：跳过补缺、
learner 当 voter、优先级降为 MEDIUM、丢失 pending、误认非 TiFlash engine。
[mutations.json](logs/task_043/mutations.json) 记录5项均 exit101、具体行为断言失败；
mutation-*.log 保存原始输出。脚本 finally 恢复正确源，再运行完整基线。

## 验证与资源

[run.py](logs/task_043/run.py) 动态从1–10原子 mkdir 领槽，记录 task/session/PID，
所有子进程继承绝对 CARGO_TARGET_DIR；正常、失败与中断均等待进程后释放自己的
锁并保留缓存。所有实际阶段使用槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[validate.py](logs/task_043/validate.py)、[commands.json](logs/task_043/commands.json)
保存确切命令、退出码、有效测试数量。完整回归用 RUST_TEST_THREADS=2，默认features。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_043 -- --nocapture` | 0 | 3通过，26场景，0失败/忽略 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 10通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1468通过，0失败，1既有忽略 |

## 只读与交付审计

开始快照 [before.json](logs/task_043/before.json) 逐文件保存 Go/模块文件哈希及
两个只读 plan.md 哈希；最终 audit.json 保存逐文件比较与 fixture 哈希。
未运行任何 Go/make/failpoint 入口。其它任务删除保留并排除提交。
编号任务仅在验证证据齐备后标记完成并删除，不提交该状态/删除。

唯一既有忽略为需要独立进程二进制的 `tso_processes`，未新增忽略。
现有 server cpu_usage deprecated 警告不影响结果。全部必需命令退出0，
槽位1锁已释放、缓存保留。Markdown相对链接与本任务diff均已检查。
[audit.json](logs/task_043/audit.json) 确认1101个Go/模块文件零差异，两个只读
plan.md未变化，fixture SHA256已记录，git diff --check通过。
日志仅清理行末空白和多余末尾空行，见
[log-normalization.json](logs/task_043/log-normalization.json)，未改变语义输出。

# 任务049：亲和组副本与领导者修复

状态：已完成。

生产路径：`pkg/schedule/checker/affinity_peer.rs`。公开
`AffinityChecker::{check, create_affinity_operator, record_op_success, recently_merged}`、
`AffinityContext` 和 `clone_region_with_replace_peer_stores`。本任务实现非 merge
Check 和成功状态记录，已亲和区域留给后续 merge 阶段。

## 依赖与来源

批次48由提交 `ed2430902`、[任务48证据](task-048.md)、canonical SplitChecker 和
既有22个checker测试确认。本次包级回归再次覆盖前序行为。
已读取两级只读plan.md、oracle-index.json、架构/依赖/移植指南。

用户明确解除Go运行oracle/运行差分准入限制。本任务依据**只读Go源码与测试契约
推导 + 实际Rust行为验证**，没有运行Go checker oracle，没有声称Go运行差分。
workspace既有etcd测试通过`go build -o <slot>/rust-etcd-fixture-PID
./scripts/rust-etcd-fixture`构建并运行只读隔离etcd helper；它不执行checker oracle，
不调用generate/tidy/failpoint，输出仅落已领取槽位。Go源码/模块内容审计仍严格相同。
[provenance](logs/task_049/source-provenance.json)记录当前来源commit、源码、fixture
及两级plan.md SHA256；Go输出哈希为null。输入、期望均冻结在
[fixture](fixtures/task_049.json)，没有用待测Rust生成期望；无字段归一化。

只读对照：`affinity_checker.go` Check/createAffinityOperator、虚拟peer替换、
best-location gate、RecordOpSuccess；`affinity_checker_test.go` 的
TestAffinityCheckerInvalidCache、TransferLeader、MovePeer、Paused、GroupState，
以及PreserveLearners、MultipleLearners、LearnerVoterConflict、
NormalizeGroupWhenPlacementRuleMismatch、TargetStoreEvictLeader、
TargetStoreRejectLeader、RegionHasBetterLocation。上述名称表示源码契约阅读，
不表示执行过Go测试。Rust用例在保留这些行为的同时使用自己的固定store/peer ID。

## 行为、错误和状态

35个固定表驱动案例覆盖：leader迁移、peer迁移、组合迁移、新leader、一个/多个
learner保留、已对齐、无组、组外与跨组边界、placement禁用、无leader、pending、
暂停、已删除region缓存清除、evict/reject/missing leader store、降级不同voters、
降级同voters观察恢复、过期观察、数量/leader不匹配归一化、learner冲突、原区域
不满足placement、更佳位置、isolation、目标位置失效归一化、ID分配失败、
组健康恢复、存储失败重试，以及直接factory入口的learner/evict/reject/missing门禁。

第2个测试额外验证虚拟placement保留shared peer ID和learner、拒绝数量/leader/
重复store错误、原region不可变，以及成功回调的limit=0、无效关联ID、双方缓存、
60秒边界和续期。时间由host显式输入。

复用真实Manager、RegionLabeler、RuleManager、ReplicaStrategy、statistics isolation
和PeerOperatorFactory。组规则由真实MemoryStore/StorageEndpoint事务创建，观察
归一化检查内存、phase=stable、leader/voters和实际持久化JSON。
成功operator逐步重放AddLearner/PromoteLearner/RemovePeer/TransferLeader，检查
最终voters/leader/learner、禁止删除learner、description、region ID/epoch、kind、
scheduler_kind及默认priority。非joint固定场景明确采用host的v2=false能力；
canonical factory既有joint行为由workspace回归覆盖。

ID分配故障返回类型化BuilderError::Allocation，不返回部分operator。
存储仅在MetaStore transact边界注入Unavailable，实际MemoryStore继续承担其它I/O；
检查故障后持久化值未变、组仍expired、无operation，解除故障后重试持久化成功。
Go在factory错误时日志并返回nil；Rust保留typed diagnostic供host处理。观察事务
错误沿用canonical Manager的Go语义，保持状态并允许下一轮重试。

Go map遍历顺序不确定；虚拟替换/角色输入以store ID升序稳定化，保留共同store、
peer ID与learner。没有把随机顺序的具体step排列伪装成Go运行输出。虚拟快照只服务
placement与位置策略，不承担完整region克隆API。宿主负责日志/metrics/完成通知。

## 失败验证和错误识别

[red-behavior.log](logs/task_049/red-behavior.log)：显式注册的真实行为测试对空实现
在transfer案例断言失败，退出101，0通过/1失败；初次API接线编译错误不作为
行为失败证据。相同测试在实现后通过。

[mutation.py](logs/task_049/mutation.py)依次注入忽略暂停、丢失learner、跳过过期
归一化、忽略当前最佳位置、忽略leader驱逐、漏记关联merge ID、提前在TTL等于到期时失效七种错误。
[结果](logs/task_049/mutation-results.json)全部由行为断言识别，exit101，无编译错误；
finally恢复生产源码，最终基线在恢复后重新执行。

TTL复核发现Go TTL.Get用expire.Before(now)，精确到期仍有效；补充该边界后，
[ttl-red.log](logs/task_049/ttl-red.log)真实断言失败（1通过/1失败，exit101），
修正为now <= expiry后重新执行全部验证。首次workspace测试因该修正主动中止
（exit130），终止并等待自己的进程组，清理锁后重跑；中止日志不计成功证据。

## 验证

[run.py](logs/task_049/run.py)每阶段原子动态领取共享槽位，记录任务/会话/PID，
子进程结束后释放；失败/中断会终止并等待进程组后清理自己的锁。所有实际阶段
领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
缓存保留。workspace测试使用`RUST_TEST_THREADS=2`，默认features。
具体argv、退出码、有效测试数见[commands.json](logs/task_049/commands.json)，
完整stdout/stderr在同目录对应`.log`。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_049 -- --nocapture` | 0 | 2测试，35固定案例及虚拟placement/成功缓存通过 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 24通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1482通过，0失败，1既有忽略 |

## 差异审查

[audit.json](logs/task_049/audit.json)核对全部1101个Go/模块文件内容（含untracked）
和Git状态、两级只读plan.md未变。没有新增Go/模块差异，没有执行make/failpoint/Go测试；workspace既有Go etcd helper如上说明。
全workspace格式化只改本任务Rust文件。Cargo.lock只增加现有workspace路径依赖和
既有async-trait依赖边，没有升级远程版本。构建警告为既有server deprecated字段。
既有其它任务文件删除排除在提交外。`git diff --check`通过。

全部验证结束后已释放自己的槽位锁，缓存保留。workspace既有忽略为tso_processes，
没有新增忽略项。日志仅清理行末空白及末尾多余空行，不改断言或退出码。

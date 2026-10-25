# 任务050：亲和组合并路径

状态：已完成。

生产路径：`pkg/schedule/checker/affinity_merge.rs`。
API：`AffinityChecker::new(host_unix_nanos)`与`check_with_neighbors`；后者返回
0/1/2个真实operator，合并双方由canonical RangeOperatorFactory构建。
原有仅修复`check`保留，共享`prepare_group`和`best_location`。
成功反馈继续使用`record_op_success`，宿主负责完成通知、时钟与邻接快照。

## 依赖与来源

批次49由提交`df6cfb23a`、[任务49证据](task-049.md)和本次包级回归确认。
复用真实Manager、placement、labeler、statistics、ReplicaStrategy、operator factory
和ControllerRecords；没有另建状态镜像或新增crate/feature/dependency。
已读取两级只读plan.md、oracle-index.json及Rust架构/依赖/移植指南。

用户明确解除Go运行oracle/运行差分准入限制。证据为**只读Go源码与测试契约推导
加实际Rust验证**，没有执行Go checker oracle，不声称Go运行差分。
[source-provenance.json](logs/task_050/source-provenance.json)固定来源commit、
只读Go/plan源码与fixture SHA256；Go输出哈希为null，无字段归一化。
[fixture](fixtures/task_050.json)输入与期望独立冻结，未从待测Rust反推。

逐段读取`affinity_checker.go` Check/mergeCheck/checkAffinityMergeTarget/
allowAffinityMerge/RecordOpSuccess，以及`operator/create_operator.go` merge factory。
Go相邻测试实际提供的源码契约包括：Basic、CheckerMergePath、NoTarget、
DifferentGroups、RegionTooLarge、AdjacentNotAffinity、NotAffinityRegion、
UnhealthyRegion、BothDirections、TargetTooBig、RespectsAffinityLimit、
DisabledByZeroConfig、AdjacentUnhealthy、PeerStoreMismatch、AdjacentAbnormalReplica、
PlacementSplitKeys、LabelerSplitKeys和WithOperatorController。
任务列出的InvalidCache/TransferLeader/MovePeer/Paused/GroupState属于前置修复门禁，
由任务49实现与本次包级回归覆盖，不能把这些名称当成本任务merge运行oracle。
以上Go测试只读核实，没有执行；Rust固定ID/边界与Go样例不完全相同。

## 行为与故障证据

1个真实Rust测试承载38个固定案例：11个merge结果、26个拒绝结果、1个类型化
ID分配失败结果；全部须通过。案例覆盖同组/不同组/组外范围/未亲和邻居、单向、
较小prev/同大小选next、启动60秒及非零启动时间、大小与keys单体/组合/等于边界、
三个禁用门禁、缓存source/目标/到期相等/过期、缺失目标、不连续范围、不健康
source/目标、不完整副本、placement禁用、暂停、已删除source、隔离位置不满足、
placement/label split keys、忽略merge_option=deny和全局size仅作启用门禁。

learner场景使用真实placement learner规则，源store4与目标store5对齐，逐步检查
AddLearner/RemovePeer及最终learners/voters，ID分配失败传播BuilderError::Allocation，
不返回部分operator。正常操作检查description、kind、epoch、双方关联ID、目标、
主动/被动MergeRegion步骤和heartbeat command。Go纯merge传入AFFINITY而非MERGE；
learner对齐时canonical factory额外加入REGION，测试保留该区别。

同组成功场景将真实passive operator在合并快照上推进至SUCCESS，通过
ControllerRecords::bury_operator触发真实成功回调，确认创建时未缓存、完成后双方
ID均缓存、重复归档幂等。其它cache案例验证此状态实际阻止后续选择，TTL沿用任务49。
合并路径不读取hot/split状态，未引入普通MergeChecker的冷却和热点门禁。

[red-behavior.log](logs/task_050/red-behavior.log)：空实现使same-group-next目标
断言失败，exit101，0通过/1失败；[red.log](logs/task_050/red.log)中的初次API编译
错误不算行为失败证据。修复后的同一注册测试通过。

[mutation-results.json](logs/task_050/mutation-results.json)记录五个独立错误：
跳过启动等待、漏掉组合size门禁、漏掉目标缓存、无视单向配置、错误operator kind，
均触发行为断言失败（exit101），并非编译失败。
探索中单独去掉group-ID比较的变异未失败，因为不同组同时被真实label split keys
拒绝；该冗余门禁实验保存在mutation-group.log，不列入错误识别成功证据。
[mutation.py](logs/task_050/mutation.py)在finally恢复源码，最终全量验证在恢复后运行。

## 验证

[run.py](logs/task_050/run.py)每阶段依次原子领取共享槽位，记录计划/任务/会话/PID，
等待子进程结束后释放自己的锁；中断和失败会终止并等待子进程组。
实际槽位及全部argv、退出码见同目录命令JSON；缓存保留。
workspace测试使用`RUST_TEST_THREADS=2`，默认features。

所有阶段实际领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
完整argv/退出码/测试数见[commands.json](logs/task_050/commands.json)。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_050 -- --nocapture` | 0 | 1测试、38案例通过 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 25通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1483通过、0失败、1既有忽略 |

既有忽略为`tso_processes`，没有新增忽略。所有子进程已结束，自己的槽位锁已释放，
缓存保留。日志仅清理行末空白和末尾多余空行，不改断言/退出码。

## 差异审查

Go及模块文件只读；未运行make、failpoint或Go checker测试。workspace既有etcd
fixture测试可能只读构建Go helper到领取槽位，不构成checker oracle。最终内容哈希、
Git状态、只读plan哈希和差异检查见audit.json。只提交本任务生产代码、测试、文档和
证据，其它任务文件的已有删除排除。没有Cargo.lock或模块依赖变化。

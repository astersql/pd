# 任务101：领导者随机扰动调度

状态：已完成。

依赖批次100通过提交 `664a70a75`、[task-100](task-100.md)及对应真实API核实：
150项包级、1699项workspace测试证据存在，本任务开始时批次100编号文件已删除。
上层两个plan.md和原主题保持只读，既有78、80–100编号文件删除未纳入本任务提交。
本任务串行执行，没有子代理。

采用用户后续授权：**只读Go源码/测试推导 + Rust真实行为和故障验证**替代缺失Go
运行oracle。没有执行Go测试、Go工具或failpoint命令，没有声称Go运行差分。
[provenance](logs/task_101/provenance.json)固定来源HEAD、基准commit、源码、准入索引及
fixture SHA-256；runtime_oracle=null。Go shuffle_leader.go、scheduler_test.go、
region_tree.go相对计划基准无差异。

## 实现与场景

[shuffle_leader.rs](../../../pkg/schedule/schedulers/shuffle_leader.rs)提供
`ShuffleLeaderConfig::{new,from_args,ranges}` 和
`ShuffleLeaderScheduler::{new,config,encode_config,is_schedule_allowed,schedule}`；
[模块说明](../../../pkg/schedule/schedulers/shuffle_leader.rs.md)记录边界，lib显式注册
生产模块和独立测试。未修改Cargo依赖或锁文件。

只读shuffle_leader.go决定：Low级目标状态过滤及specialUse过滤、先选随机store再抽
follower、pending/down整区域拒绝、真实transfer-leader builder、ADMIN|LEADER、Low。
init.go复用getKeyRanges；scheduler_test.go的TestShuffleLeader作为kind和transfer-in
限流的源码依据（未执行）。core/region_tree.go的RandomRegions明确空ranges覆盖全树。

[fixture](fixtures/task_101.json)固定24条源码推导场景，在classic/conf-change-v2心跳
模式各执行一次，共48条真实行为案例：正常、缺region、pending/down、全不健康、拒绝
leader标签、暂停目标、无合法目标、源暂停迁出、range不相交/完整包含/部分包含、learner、
witness、disconnected、真实transfer-in预算耗尽、placement拒绝/允许、随机最后目标、
抽中当前leader所在store后无follower且不重试、空ranges、specialUse、offline、空集群。

归一化：候选store按ID排序；随机回调固定首或末索引，范围shuffle不变；区域固定ID1、
[a,b)、3个peer，leader在store1；时间1000000000000ns。测试没有忽略错误或操作字段。
真实BasicCluster索引、StoreStateFilter、RuleManager、PeerOperatorFactory和Operator
共同运行；只有时钟、随机输入及MetaStore I/O边界受控。

3项显式注册测试：

- `task_101_behavior_cases`：48案例；验证操作数量、typed LeaderNotAllowed、description、
  ADMIN|LEADER、Low、单步TransferLeader、from/to、空替代目标、confver=0、leader影响
  -1/+1、两模式心跳peer、应用leader后操作成功。正常操作也证明不调用peer分配。
- `task_101_config_and_limits`：百分号/加号/十六进制JSON编码、往返、坏参数/JSON拒绝、
  默认全范围；5组严格限额边界含0和u64::MAX。
- `task_101_config_storage_faults`：使用真实SchedulerConfig、StorageEndpoint、MemoryStore
  保存和重建恢复；read/transact返回Unavailable时保存/更新/重载失败且旧配置保留；
  坏JSON重载同样保留旧值。不为Go没有的专用reload新增scheduler API。

## 失败与通过验证

initial记录未实现API导致编译失败，exit101；不以其代替行为红灯。
focused记录placement测试装配缺少匹配store；focused2记录NodeState枚举修正；这两项
是测试装配问题。focused3记录空ranges预期错误，随后核对Go RandomRegions修正fixture
为全树采样（没有修改生产行为）。focused4的3项/48案例通过。

[red_behavior.py](logs/task_101/red_behavior.py)故意把Low优先级改为High，先cargo fmt，
再运行同一task_101_behavior_cases，priority断言失败，0通过/1失败，Cargo exit101；
脚本预期失败返回0，finally逐字节恢复生产实现。最终验证在恢复后重新执行。

所有Cargo命令和调用Cargo的脚本经[run.py](logs/task_101/run.py)原子mkdir动态领取
仓库共享槽位1–10，owner记录计划/任务/会话/PID，finally释放自己的锁并保留缓存。


## 最终验证与审查

各阶段实际槽位均为 **1**，绝对 CARGO_TARGET_DIR：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[final.json](logs/task_101/final.json)记录确切顺序命令和整体exit0，`&&`链证明下列各项
exit0；最终阶段设置CARGO_BUILD_JOBS=1、RUST_TEST_THREADS=2。各阶段JSON记录命令、
退出码及环境，对应log.gz保留完整stdout/stderr；[counts.json](logs/task_101/counts.json)
按最终日志汇总有效数量。

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅本任务Rust差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_101 -- --nocapture | 0 | 3项通过；48条行为案例 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 153通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | workspace元数据通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过；既有cpu_usage弃用warning |
| cargo test --workspace --locked | 0 | 1702通过/0失败/1既有忽略，含2项文档测试 |

[audit.json](logs/task_101/audit.json)确认1103个Go/模块及只读plan文件起止SHA-256
一致，无新增Go/模块文件，相关Git状态为空；git diff --check退出0。文档本地链接检查
通过，Cargo.lock及两个plan.md未改。最终所有Cargo/使用槽位产物进程均已结束，
slot-1.lock已释放，缓存保留。没有必需验证缺口（使用明确授权的源码推导证据口径）。

提交只包含本任务实现、独立测试、模块接线、说明和证据；编号任务状态和删除不提交，
不暂存其它任务既有删除。完成后删除任务101编号文件。

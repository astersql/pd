# 任务076：领导者驱逐配置事务

状态：已完成。

批次75依赖已满足：提交 `5700d80e3` 及[任务075证据](task-075.md)提供
canonical `EvictLeaderConfig/EvictLeaderScheduler`、真实 core 暂停引用计数、
filter/operator 和 MetaStore 接口。源码准入读取两级只读计划、ADR、移植指南和
oracle-index.json。沿用用户明确授予整个计划的豁免：**只读Go源码/测试推导 +
Rust真实行为及故障验证**；没有Go运行oracle或运行差分，不伪造Go输出。

生产路径：[evict_leader_config.rs](../../../pkg/schedule/schedulers/evict_leader_config.rs)，
[领域边界和限制](../../../pkg/schedule/schedulers/evict_leader_config.rs.md)。
在同一 canonical scheduler 上增加 `EvictLeaderConfig::from_args`、
`evict_store_id_from_float`、`encode_evict_config`、`update_evict_config`、
`delete_evict_store`、`reload_evict_config`；lib.rs显式注册生产模块和独立测试。
仅调整既有配置的可见性及缺失/零batch兼容默认，不新增依赖、features或运行时。

## 来源与冻结

来源基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[provenance](logs/task_076/provenance.json)记录Go源码/测试、fixture及Rust源码SHA256，
Go输出及其哈希明确为null。检查的Go文件均和来源基准一致。
[固定输入及期望](fixtures/task_076.json)为独立源码推导，未从Rust运行输出反推。
排序只用于Go map未定义的迭代顺序；本任务没有随机/时钟字段归一化。

- `evict_leader.go` 的parseUpdateStoreIDs/updateConfig/applyStoreIDs：单/多ID、去重、
  batch校验、空ranges保留旧值，新store默认全键空间，单次保存及暂停失败/保存失败回滚。
- 同文件delete/reloadConfig：最后store移除回调、NotFound特例、普通错误恢复内存/暂停、
  保存后的空配置不被回调回滚重新写入；reload零batch默认3和store集差异暂停/恢复。
- `init.go` 的EvictLeader注册解码：字符串uint64、多store逗号参数、禁止多store附范围；
  `utils.go` 的getKeyRanges和pauseAndResumeLeaderTransfer：URL字节范围、忽略奇数尾、
  retained store不增加暂停引用；复用前序Rust canonical工具函数。
- 相邻TestEvictLeader、TestEvictLeaderWithUnhealthyPeer、TestConfigClone、TestBatchEvict、
  TestEvictLeaderBatchLimit主要覆盖调度或batch；本任务额外只读审查
  TestEvictLeaderInvalidBatchKeepsLeaderTransferState、UpdateConfigStoreIDsValidation、
  BatchPreservesExistingRanges、BatchCopiesExplicitRangesPerStore、EmptyRangesPreservesExisting、
  BatchRollbackPreservesExisting、BatchRollbackOnMidLoopPauseFailure、MultiStoreCreationArgs、
  DeleteWithFailedCallback和DeleteWithSaveFailure。未裸跑failpoint依赖Go测试。

兼容边界如实保留：沿用前置领域schema，reload接受的有效batch为1–100（零/缺失默认3），
损坏/不兼容存储数据保留原配置；Go未校验的任意persisted int不属于该schema。
Go的float64(MaxUint64)舍入为2^64，其超范围转换无法保证平台无关语义；Rust拒绝
不可表示的2^64边界，字符串创建API仍接受精确u64::MAX。这不是已验证的Go运行等价声明。

## 测试驱动与故障结果

`task_076_behavior_cases`：23/23固定案例（8正常、15拒绝），验证配置batch、store集合、
暂停状态、保存字节、clone快照、reload及clean。覆盖single/multi/dedup、空/显式范围、
batch-only/null、奇数范围、zero ID、互斥参数、小数/负数/越界/非法ID、batch边界和
非法范围类型/转义。无新忽略测试，配置读写使用真实MemoryStore/StorageEndpoint。

`task_076_transactions_and_faults`：新增暂停后在MetaStore I/O边界停住并取消，
验证本次暂停释放；真实缺失store999使批量暂停中途失败；MetaStore读写Unavailable
验证保存失败不发布batch/ranges，重试可成功。核对原存储和已有IN/OUT独立暂停引用，
删除保存失败、非最后删除、缺失store、最后store普通回调失败/NotFound/成功及持久化差异。
正常I/O委派canonical MemoryStore，只有外部I/O边界可控制，不伪造业务结果。

`task_076_args_reload_and_boundaries`：非法创建参数、精确最大uint64、逗号去重、非有限/
小数/负数/超范围浮点ID；损坏JSON/batch重载失败保留原配置及暂停；零/缺失batch默认3，
store1→2暂停差异、重复reload不增加暂停引用，新增store999暂停失败返回具体错误且发布，
缺失持久化键保留原配置。

[red-api](logs/task_076/red-api.log.gz)记录初次API缺失编译失败（同时含随后修复的重复导入），
退出101；它不作为行为失败证据。
[red-behavior](logs/task_076/red-behavior.log.gz)临时把暂停回滚循环改为take(0)，
同一故障测试检出store2暂停泄漏，2通过/1失败，退出101；随后恢复正确实现。
[focused](logs/task_076/focused.log.gz)最终3项通过，23/23固定更新案例通过；
先前focused-first/second为增加精确ID解析、取消和reload边界验证过程的成功记录。

## 验证、共享槽位与审计

命令argv、退出码及环境逐项保存于[日志目录](logs/task_076/)的同名JSON。
run.py从根目录通过原子mkdir动态领取槽位，所有阶段实际使用 **槽位1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常、失败和中断路径等待子进程退出再清理自己的owner/锁，保留编译缓存。
Rust修改后先运行cargo fmt --all，再进行下列验证。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_076 -- --nocapture` | 0 | 3通过、0失败；23/23固定案例及独立故障/边界测试 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 38通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1587通过、0失败、1既有忽略 |

[审计](logs/task_076/audit.json)核对1101个Go/模块文件清单及SHA256（含untracked扫描），
Go状态与开始时一致，两份plan.md哈希不变。`git diff --check`退出0。
其它任务原有删除/修改不属于本提交，根/包Cargo.toml及Cargo.lock未变化。

[counts](logs/task_076/counts.json)按实际test result行汇总，含2项文档测试；唯一既有忽略为
`tso_processes`独立进程测试，没有新增忽略。所有原始日志按[编码说明](logs/task_076/log-encoding.json)
保留字节gzip压缩。最终槽位1锁已释放、缓存保留。文档链接及提交范围已审查。

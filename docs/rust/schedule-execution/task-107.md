# 任务107：调度器增删、暂停和恢复

依赖106已核实：提交 `45c38e6d2`、[task-106](task-106.md)包含真实18类工厂、
包168测试及workspace1717测试证据。当前任务串行执行，不修改两级plan或原主题。
本任务按用户后续授权，以只读Go源码推导 + Rust真实行为/故障测试替代缺失的Go
运行oracle；没有运行Go工具、测试或failpoints，也没有宣称获得Go运行差分。
[provenance](logs/task_107/provenance.json)固定来源commit、计划基准及源码/fixture哈希。
运行oracle为null；不从Rust实现反推冻结期望。

## 生产路径与边界

[scheduler_control.rs](../../../pkg/schedule/schedulers/scheduler_control.rs)提供
`SchedulerController::{add,remove,exists,get,names,handler,handler_names,is_disabled,
pause_or_resume,is_paused,delay,reload}`及`ScheduleControl`。
复用106真实RegisteredScheduler、真实BasicCluster和StorageEndpoint；同一owner保留
调度器索引、handler索引、cluster配置及暂停计时。局部lib显式注册生产模块与独立测试；
没有Cargo依赖、feature或Go变更。HTTP及后台循环不在本任务范围。

新增在prepare和保存成功后发布；prepare失败回收已取得的store暂停引用。
handler restore先检查历史，只删除此次失败创建的孤立配置。remove按Go顺序持久化
cluster配置、禁用并删除独立配置、clean并移除索引；中途失败保留可重试索引，前序
已修改的cluster配置不回滚。default disable条目可由后续add重新启用。
reload复用具体策略的规范方法；读/解码失败保留旧配置和暂停时间，重复reload不会
重复取得store暂停引用。Go继承Base空reload的五类策略保持空操作。
详见[领域说明](../../../pkg/schedule/schedulers/scheduler_control.rs.md)，含hot快照重建、
宿主串行执行future以及独立pending/运行循环边界。

## 源码依据和测试

Go `scheduler_controller.go` 的 Add/RemoveScheduler[Handler]、PauseOrResumeScheduler、
ReloadSchedulerConfig、IsSchedulerExisted、GetSchedulerHandlers、GetDelayAt/Until；
`server/config/persist_options.go` 的 Add/RemoveSchedulerCfg；各具体策略Prepare/Clean/
ReloadConfig用于生命周期接线。已有Go控制层rollback及handler-only reload测试作只读
场景参考，未声称执行。只读oracle-index未提供本任务完整运行矩阵，按授权如实使用源码推导。

[fixture](fixtures/task_107.json)冻结5行TTL场景：Unix100暂停5秒、104仍暂停、105
到期、TTL0和-1恢复。时钟由显式Unix秒输入控制，不归一化或忽略错误/状态差异。
另复用106冻结18类有效factory参数，矩阵两个mode共36条生命周期场景。

4个独立有效测试：

- `task_107_behavior_cases`：add/remove、重复、不存在、TTL5行、all、default disable、
  保留其它cluster配置、handler索引、handler-only reload错误及未bootstrap错误。
- `task_107_reload_and_prepare_rollback`：真实evict store1暂停；坏JSON reload保留配置
  和计时；有效reload转移至store2、重复reload、remove完整释放引用；普通及handler
  部分prepare失败（store999不存在）均释放store1且不发布索引/孤立配置。
- `task_107_all_factories_and_indexes`：18类×2mode=36条真实生命周期，包括普通模式
  reload保留暂停及各类真实prepare/clean；handler非空优先及空索引fallback。
- `task_107_storage_faults`：真实MemoryStore之read/transact I/O边界Unavailable；add
  保存失败清理、reload读取失败保留配置、remove cluster保存/独立配置删除两阶段失败
  保留索引和引用并可重试；handler历史保留及读取失败不发布；default禁用持久化失败
  后保留修改的cluster配置，remove重试和add重新启用。

初始缺API红灯见initial（exit101）；focused记录实现装配的借用/类型编译错误，
focused2/expanded记录逐步绿灯。补充[行为变异脚本](logs/task_107/red_behavior.py)
分别把TTL严格小于改为小于等于、在reload清空暂停截止时间，同一聚焦测试均产生
真实断言失败exit101；[red-results](logs/task_107/red-results.json)逐项确认。
脚本finally恢复源码；脚本exit0表示两个错误行为都被测试发现。

## 最终验证

最终命令、退出码、实际槽位及绝对target见[final.json](logs/task_107/final.json)，
完整stdout/stderr见[final.log.gz](logs/task_107/final.log.gz)，测试统计见
[test-counts](logs/task_107/test-counts.json)。命令顺序如下，`&&`确保前序失败时停止：

```sh
cargo fmt --all
cargo test -p astersql-pd-schedule-schedulers --locked --lib task_107 -- --nocapture
cargo test -p astersql-pd-schedule-schedulers --locked
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

每个阶段由[run.py](logs/task_107/run.py)原子mkdir动态领取共享槽位；实际均为1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
最终阶段`CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2`，Cargo及所有子进程继承设置。
正常/失败/中断均先等待进程结束再释放自有锁，缓存保留，无其它target-dir。

最终全部命令退出0：聚焦4测试、包172测试、workspace1721测试通过
（含2个compile-fail文档测试，既有ignored=1，failed=0）。两项行为变异均被断言检出。
[只读审计](logs/task_107/readonly-check.json)确认1101个Go/模块文件哈希与文件清单一致，
Go及两份只读plan均无差异；git diff --check通过。自有槽位锁已释放，缓存保留。

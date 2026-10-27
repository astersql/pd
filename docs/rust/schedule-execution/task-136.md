# 任务136：配置恢复和在途操作语义验收

状态：已完成。依赖135提交 `aca52c9a2` 及[闭环证据](task-135.md)已核实。
新增[集成测试](../../../tests/rust-compat/schedule_restore_test.rs)、显式
`[[test]] name="schedule_restore"`、4条既有workspace依赖边和本任务证据；
已有生产API满足本故事，无新增生产实现。两级 plan.md 保持只读。

## 证据边界与来源

按用户后续授权，采用**只读Go源码/测试推导 + Rust真实行为及故障验证**，
明确替代本任务缺失的Go运行oracle；未运行或宣称新的Go运行差分。
[来源清单](logs/task_136/provenance.json)冻结Go来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、7份源码/测试哈希、135证据、
准入oracle索引和[固定输入/期望](fixtures/task_136.json)哈希。
所用Go文件均与该基准相同。生命周期测试使用暂停的Tokio时钟，Unix时间及随机选择显式受控；
operator/pending保留真实单调时钟，立即检查未超时的STARTED状态，无输出字段归一化。
没有从待测Rust输出生成期望。

源码契约对应：

- `Coordinator.InitSchedulers`：独立scheduler JSON优先于旧args；disabled条目
  保留但不实例化；单项创建/保存失败跳过并清理配置；最终整份配置保存失败不阻止初始化。
- `Controller.PauseOrResumeScheduler`只设置实例内存delay，没有持久化；重建后不延续pause。
- `NewCoordinator`新建operator controller；controller构造函数新建waiting、records、
  counts等内存状态，没有恢复未完成operator的路径。停止不把它伪造为SUCCESS或CANCELED。
- `newBaseHotScheduler`新建空regionPendings；热点pending由真实operator状态计算影响量。
- `Coordinator.Stop`取消，`RunUntilStop`等待scheduler与worker退出；只有join后才替换宿主实例。

Rust的`HotLoadProjection`由宿主显式持有，与scheduler算法的所有权分离。
测试验证该真实类型在旧宿主上产生非零影响量、丢弃宿主后新建为空；没有声称
`init_schedulers`会隐式清空任意外部对象。测试的fencing边界是取消/join后交接，
没有实现或宣称分布式租约、leader任期或持有旧Arc者的跨实例写入fencing。

## 实际路径与有效案例

路径为 `Coordinator::new` → `init_schedulers`/`restore_schedulers` →
`SchedulerRegistry`/`SchedulerController` → canonical `GrantLeaderScheduler::schedule`、
`ControllerAdmission::add_operators/get_op_influence`、`HotLoadProjection`，并通过
`Coordinator::run` → `CoordinatorRun::wait`验证真实后台生命周期。

使用真实BasicCluster、三副本RegionInfo、RuleManager、StorageEndpoint和MemoryStore。
复用任务115的宿主装配方式，PatrolHost将检查、split/scatter及统计操作委托给真实组件；
没有mock cluster或复制生产恢复逻辑。FaultStore仅在MetaStore读写I/O边界注入错误。
所有cache均正常shutdown，所有run均取消并join。

`task_136_behavior_cases`有2个模式案例（classic运行、handler-only）：

- 在整份配置grant args目标1与独立持久化目标3冲突时，恢复前后真实规划均为region1、
  LEADER kind、TransferLeader 1→3；完整scheduler配置字节在重建前后相等。
- disabled的balance-leader配置保留，handler不存在。classic的600秒pause在重建后为false、
  delay为(0,0)；handler-only没有运行scheduler。
- 旧operator为STARTED，活跃leader影响量为源-1、目标+1；hot pending实际loads为-10/+10。
  新coordinator无operator/活跃影响量，新宿主hot pending为空且统计无pending sum；
  新operator可以重新准入且与旧Arc不同。旧实例停止不合成终态。

`task_136_persistence_failure_recovery`有5个阶段案例：独立配置写失败且Prepare回滚；
清理后的持久化配置重建不复活scheduler；显式重新配置成功；整份配置写失败后初始化
仍成功、存储维持旧配置；重建读错误不初始化，I/O恢复后重新初始化成功并保持独立目标3。

`task_136_stop_join_before_replacement`有1个交接案例：旧Patrol实际阻塞在途检查，重复
取消后wait仍未完成且Prepare仍生效；释放I/O/检查边界后join成功、释放store pause；
推进时钟120秒不再执行旧scan。随后才重建，恢复同一持久化evict配置、重新Prepare，
且不恢复旧operator。合计**3个测试、8个有效模式/阶段案例**，没有新增忽略测试。

## 红/绿验证及基线

[compile](logs/task_136/compile.json)记录首次缺少锁文件依赖边，
[lock](logs/task_136/lock.json)记录离线锁更新和初次API接线错误；这些不作为行为失败证据。
离线更新引入的pulldown-cmark漂移已还原，最终无远程依赖版本变化。
固定输入采用显式完整keyspace范围（空range列表的源码语义是“不选择region”）。
[green](logs/task_136/green.json)确认现有生产组件已满足新增故事。

[变异脚本](logs/task_136/mutation.py)对生产Rust临时施加两个错误，finally逐字节还原：

- [red-restore](logs/task_136/red-restore.json)：跳过独立配置恢复；同一目标退出101，
  2通过/1失败，真实grant决策无法生成期望的1→3操作。
- [red-cleanup](logs/task_136/red-cleanup.json)：省略停止清理；同一目标退出101，
  2通过/1失败，join后store的IN pause未释放。

因此失败证据是实际行为断言，而不是缺包、缺注册、编译错误或零测试。
生产文件最终与开始时相同。还原后完成以下完整基线验证；之后以
“格式化→变异验证”顺序重放同样两个变异，再在恢复原始字节后通过
[聚焦测试与格式检查](logs/task_136/post-mutation.json)。未永久更改生产文件，
完整workspace基线与最后聚焦校验采用相同源码。

[最终命令与完整压缩stdout/stderr](logs/task_136/final.json)保存命令、退出码、
有效测试摘要及每份日志哈希；[最终审计](logs/task_136/audit.json)重新核实全部结果。

| 命令 | 退出码及有效结果 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo test -p astersql-pd-compat-tests --locked --test schedule_restore -- --nocapture` | 0；3通过，8有效案例 |
| `cargo test -p astersql-pd-compat-tests --locked` | 0；26通过，1既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0 |
| `cargo test --workspace --locked` | 0；1851通过，0失败，1既有忽略 |

全部Cargo调用经[槽位脚本](logs/task_136/verify.py)原子mkdir动态领取共享槽位。
本次各阶段实际槽位为1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划、任务、会话、PID；正常/错误/信号中断均先等待或终止并join子进程组，
再删除自己的owner并rmdir释放锁，保留缓存。

workspace现有etcd测试复用任务030已保存二进制，SHA256为
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`。
[Go命令拦截器](logs/task_136/reuse-go/go)仅复制该固定二进制，拒绝真实Go命令；
[复制记录](logs/task_136/go-reuse.jsonl)保留每次使用。没有执行Go测试、生成器、
模块整理、格式化或failpoint注入。

[审计脚本](logs/task_136/audit.py)核对1101份Go/模块文件集合、内容与Git状态、
只读计划、源码provenance、日志哈希、链接、无远程依赖漂移、变异恢复与槽位释放；
`git diff --check`通过。其它任务已有删除未纳入本任务提交。

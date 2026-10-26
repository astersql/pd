# 任务084：慢趋势驱逐恢复

依赖83已核验：提交 `7ca4de1d0`、[task-083](task-083.md) 的真实候选/API、
5个聚焦、71个包级、1620个workspace测试证据。读取整体/包级计划及oracle-index。
按用户对整个计划的后续授权，使用**只读Go源码/测试推导 + Rust真实行为和故障验证**
替代缺失的Go运行oracle；**本任务没有Go运行输出或运行差分**。

生产实现：[slow_trend_recover.rs](../../../pkg/schedule/schedulers/slow_trend_recover.rs)，
[领域边界](../../../pkg/schedule/schedulers/slow_trend_recover.rs.md)。公开API为
SlowTrendConfig、SlowTrendScheduler::{observe,schedule,ready_for_recovery,next_interval,
is_schedule_allowed,update_recovery,reload,prepare,clean}及SlowTrendError。
复用83候选、真实core/store控制、StorageEndpoint/MemoryStore、placement和leader builder。
不引入依赖/feature，不修改Cargo.lock或只读计划、来源主题。

## 期望来源与失败证据

基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe` 的Go生产/测试文件与当前源码相同，
[provenance](logs/task_084/provenance.json) 保存SHA-256及fixture摘要。
TestEvictSlowTrendBasicFuncs 推导捕获计时/历史；TestEvictSlowTrend/V2 推导
捕获、等待更新、驱逐、取消、恢复；TestEvictSlowTrendPrepare 推导重建标记。
持久化失败、rollback/reload、interval及精确边界来自生产分支推导，
不声称上述Go测试已运行或包含全部故障案例。

[fixture](fixtures/task_084.json) 预先冻结5组恢复案例：阈值前1纳秒、精确阈值、
阈值后、过慢、10%容差。capture=1s，eviction=2s，RecoverySec=1800，
所以恰好1801s恢复，证明计时从捕获而非驱逐开始；逐字段比较evicted id及core标记。
时间/随机为显式输入；capture的两个读数均用该tick的now；没有丢弃操作/错误字段。

[red-api](logs/task_084/red-api.log.gz) 缺失API编译失败，退出101，不视为行为红灯。
[red-behavior](logs/task_084/red-behavior.log.gz) 将恢复 `>=` 临时改成 `>` 后，
同一fixture的exact期望0实际1，真实操作器恢复断言也失败：2通过/2失败，退出101。
随后逐字节恢复生产文件，增加取消清理保证并重新fmt，最终5通过/0失败。

## 行为与故障

显式注册 [task_084_test.rs](../../../pkg/schedule/schedulers/task_084_test.rs)：

- task_084_behavior_cases：5/5固定恢复案例，真实捕获/心跳观察/驱逐/恢复/core标记。
- task_084_persistence_faults_and_rebuild：驱逐保存失败保留内存而无core标记；
  RecoverySec更新失败回滚；保存/解码/重建/Prepare；重复和不存在节点Prepare错误；
  clear失败清空内存、保留旧存储但解除core标记并更新时间；再次重建后成功清理；
  三组坏JSON；缺失驱逐节点清理；重启零时间的Duration饱和边界。
- task_084_candidate_lifecycle_reload_and_interval：亚秒截断、变快取消保留历史、
  缺失候选类型化错误、reload incoming pause引用/失败/历史保留、Prepare及Removed恢复、
  exponential和zero interval、忽略旧interval。
- task_084_real_operators：真实150个region/3个store、batch=3、无可用目标后恢复；
  检查3个去重操作器的描述、Leader kind、urgent优先级、单步TransferLeader、
  from=1/targets=[2,3]、confver=0、leader影响-1/+1、classic/raft-v2命令及应用成功；
  变快恢复后无操作器。
- task_084_cancelled_storage_boundary：在实际MetaStore事务I/O暂停后取消，
  捕获驱逐保留内存但未获得core标记；clean取消由Drop guard解除core并标记历史，
  存储保留上次成功配置。取消语义是Rust异步资源边界保证，不伪造Go对照。

故障后端只在read/transact边界注入Unavailable/pending，正常路径委派真实MemoryStore。
缺失候选或驱逐节点在Go可能nil解引用；Rust分别返回类型化错误或安全清理。
其余故障顺序遵循Go：set/clear失败不回滚列表，策略更新失败回滚，clear失败仍释放core。

## 验证

[run.py](logs/task_084/run.py) 每阶段按原子mkdir动态领取仓库共享槽位1–10，
记录plan/task/session/PID，结束/失败/中断等待子进程后删除自身锁，保留缓存。
本任务各阶段实际均领取**槽位1**，绝对CARGO_TARGET_DIR为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
每条确切命令、退出码和环境见同名JSON，stdout/stderr合并gzip保存。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 只保留本任务Rust差异 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_084 -- --nocapture` | 0 | 5通过/0失败/0忽略；5固定恢复案例及真实操作器/故障 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 76通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过；已有cpu_usage弃用警告 |

第一次 `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked`
退出101：1588通过/1失败/1既有忽略，失败为未修改的
`syncutil::ordered_single_flight_test::singleflight_randomized`，在register辅助函数
断言首次poll必须Pending（ordered_single_flight_test.rs:85）。该并发测试可能在
poll返回前已完成。保留[first-run](logs/task_084/workspace-test.log.gz)。
单独 `cargo test -p astersql-pd-syncutil --locked --lib singleflight_randomized -- --nocapture`
立即1通过/0失败，退出0，见[retry-syncutil](logs/task_084/retry-syncutil.log.gz)。
未修改syncutil或放宽测试断言，随后重新执行完整workspace测试。

第二次完整 `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked`
退出0：**1625通过/0失败/1既有忽略**，包括2个文档测试。见
[workspace-test-retry](logs/task_084/workspace-test-retry.log.gz)、
[逐组计数](logs/task_084/counts.json)。既有忽略为tso_processes。

`python3 docs/rust/schedule-execution/logs/task_084/audit.py` 与 `git diff --check`
均退出0；[audit](logs/task_084/audit.json) 比较1119个Go/模块/plan文件的前后
SHA-256（含untracked扫描），全部一致，无新增Go/模块文件。
[开始快照](logs/task_084/before.json.gz)及初始Git状态保留；其它任务78/80/81/82/83
已有删除未纳入本提交。来源主题无diff。workspace仅复用只读rust-etcd-fixture源码，
构建二进制和WAL位于所持槽位，不注入failpoint、不写Go/模块文件。

必需验证均已获得退出0的最终证据；实际槽位1的自身锁已释放，缓存保留。
已审查生产/测试/fixture/接线差异与本文链接。状态：已完成。

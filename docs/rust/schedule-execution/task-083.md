# 任务083：慢趋势候选选择

依赖82已核验：提交 `cb4d6b4b5`、[task-082](task-082.md) 的真实
core/storage/operator API 与4个聚焦、66个包级、1615个workspace测试证据齐备。
按用户对整个计划的后续明确授权，使用 **只读Go源码/测试推导 + Rust真实行为和故障验证**
替代缺失的Go运行oracle。已读取oracle-index.json；**没有本任务Go运行输出或运行差分**。
整体/包级plan.md及来源主题始终只读，未改Cargo依赖/锁文件或增加feature。

生产路径：[slow_trend_candidate.rs](../../../pkg/schedule/schedulers/slow_trend_candidate.rs)，
[领域说明](../../../pkg/schedule/schedulers/slow_trend_candidate.rs.md)。公开API：
choose_slow_trend_candidate、slow_trend_stores_updated、slow_trend_slower_than_others、
slow_trend_faster_than_others、SlowTrendCandidates::{capture,pop,mark_recovered}、
SlowTrendCandidate::captured_secs。候选逻辑使用真实BasicCluster/StoreInfo/SlowTrend；
驱逐持久化与操作器由后继任务接入，本任务不复制这些实现。

## 期望来源与测试驱动

来源基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`，Go生产和测试文件均与该基准相同，
SHA-256及fixture摘要见[provenance](logs/task_083/provenance.json)。
TestEvictSlowTrendBasicFuncs 推导capture/pop/历史；TestEvictSlowTrend及V2推导候选、
等待心跳、变快取消及raft-v2行为。epsilon、整数人数阈值、120秒边界、缺失数据及
生命周期额外用例逐分支取自Go生产源码，并不宣称Go suite运行覆盖。

[fixture](fixtures/task_083.json) 为14个手工冻结候选输入/期望：正常、少于3节点、
cause/result严格epsilon、无QPS下降、不更慢、受影响阈值、v2在120及120.999999999秒、
121秒过期、classic/无历史拒绝、v2受影响不足、阈值取整。直接配置参数的阈值案例
包含ratio>1，用于验证源函数截断/拒绝分支，不表示上层配置允许该值。
初次执行发现一条手工阈值案例误将3*1.1截断为4，已核对Go整数转换后将输入改为
3*1.34=4.02使阈值确为4；期望0未从Rust输出生成。

独立注册 [task_083_test.rs](../../../pkg/schedule/schedulers/task_083_test.rs)：

- task_083_behavior_cases：14/14固定候选案例，输出含案例数。
- task_083_updates_and_comparison_boundaries：6个人数(0..5)心跳边界、精确等时、
  移除/Removing/Unknown节点、4个快比较值(11、11.000001、0、NaN)、
  missing/zero数据及多个候选，检查真实registry变化。
- task_083_capture_pop_and_time：首次历史、两个独立时间读数、亚秒/1秒截断、
  markRecovered、pop(true/false)、空pop覆盖历史。
- task_083_candidate_heartbeat_flow：捕获后等待、更新真实StoreInfo心跳后就绪、
  更新真实统计后变快并取消，检查last仍指向先前确认候选。
- task_083_lifecycle_and_missing_reports：5个节点状态、缺失候选报告、
  peer值恰为epsilon、Removed节点仍计入分母。

该层没有外部I/O和随机输入，因此故障是缺失/NaN统计及歧义候选，未虚构持久化故障。
时间输入为显式Unix纳秒、capture/recover两次读数；宿主使用不回退时钟。
负间隔防御性归零，不宣称复刻Go负浮点转uint64的平台行为；不忽略任何候选结果。
默认features同时覆盖classic/raft-v2参数分支。

[red-api](logs/task_083/red-api.log.gz) 为缺失API编译错误，退出101，不作为行为红灯。
[red-behavior](logs/task_083/red-behavior.log.gz) 将cause上升谓词临时反向：
normal期望1实际0，3失败/2通过，退出101；随后逐字节恢复生产文件并重新格式化，
同一测试5通过/0失败。这证明测试能识别实际错误行为。

## 验证

[run.py](logs/task_083/run.py) 每阶段从仓库根原子mkdir动态领取1–10槽位，记录
plan/task/session/PID；异常终止先等待子进程退出再清理自身锁。实际全部领取
**槽位1**；绝对CARGO_TARGET_DIR为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。缓存保留。
每条命令、退出码、槽位及环境在同名JSON，原始stdout/stderr合并gzip保存。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务Rust变更 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_083 -- --nocapture` | 0 | 5通过/0失败，14固定候选案例 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 71通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1620通过/0失败，1既有忽略 |
| `git diff --check` | 0 | 通过 |

[counts](logs/task_083/counts.json) 为原始stdout的逐组汇总，含2个文档测试，既有忽略为
`tso_processes`。[audit](logs/task_083/audit.json) 确认1119个Go/模块/计划文件前后
内容哈希完全一致，无新增Go或模块文件；只读原主题无diff。完整开始快照保存在
[before](logs/task_083/before.json.gz)，保留初始status，已有78/80/81/82任务删除未纳入提交。
workspace沿用只读rust-etcd-fixture Go源码，二进制/WAL均写入所持槽位，不注入failpoint，
不改Go或模块文件。所有必需检查退出0，槽位1自身锁已释放，缓存保留。
源码、测试、fixture、接线和本文相对链接均已审查。

状态：已完成。

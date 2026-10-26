# 任务069：调度负载比较与待定权重

状态：已完成。

前置68提交 `639c3b1a1` 提供真实 RangeCluster/RetryQuota/get_count_threshold；
其包级16项、聚焦3项及全仓1565项成功记录已审阅。本次包级再次覆盖这些接口。

生产实现：[load_compare.rs](../../../pkg/schedule/schedulers/load_compare.rs)，
[API与边界说明](../../../pkg/schedule/schedulers/load_compare.rs.md)。
新增 rank、反转、lexicographic/slice、预测 min/max/diff 比较；
PendingInfluence 保留真实 Arc<Operator>；权重刷新操作状态并区分影响归零与回收；
leader transfer 集合差分复用真实 core 的方向引用计数，错误留给宿主处理。
只增加既有 workspace statistics 路径依赖，未修改远程依赖版本。

## 来源及冻结期望

本任务遵循用户后续授权，是 **只读 Go 源码/测试推导 + Rust 行为实测**。
没有本任务的 Go 运行输出或 Go/Rust 运行差分。
[provenance](logs/task_069/provenance.json) 保存来源 commit、源文件与 fixture SHA256，
Go binary/output SHA256 为 null。已阅读 oracle-index 的既有运行路线，它们不覆盖
本任务案例，没有借用其结果。两份 plan.md 与来源主题只读。

`utils.go` 定义比较器和 pause/resume 集合差分；相邻 `utils_test.go` 的
TestRetryQuota、TestGetCountThreshold 覆盖前置68的配额和阈值，不能证明本次行为。
权重真实算法来自 `hot_region.go::calcPendingInfluence`，状态刷新顺序来自
`operator.go::CheckAndGetStatus`。`hot_region_test.go::TestGCPendingOpInfos` 提供
活动、取消和延迟GC契约。全部只读查阅，没有运行 Go 或开启 failpoint。

[fixture](fixtures/task_069.json) 冻结37项：7项rank、30项pending。
rank 使用向零截断的 `value/10`：11和19相等，-19和-11相等，-9和9相等。
30项pending覆盖Created/Started/Success/Canceled/Replaced/Timeout/Expired各4个
elapsed=0/9/10/11ns，窗口10ns；另2项零窗口Success/Canceled。
活动状态恒为(1,false)；成功仅age<window保留1；失败立即0但仅age>=window可GC。
无字段忽略或浮点容差；显式时钟控制elapsed，测试期望未由Rust待测实现生成。

## 实测与故障

独立注册5项测试：

- `task_069_behavior_cases`：7项冻结rank，以及反转、空比较器、平局后比较count、
  首个非零结果短路并保留非单位返回值、四维逐项min/max/diff、预测比较顺序、独立count投影边界。
- `task_069_pending_lifecycle`：30项冻结生命周期，真实Start、Check heartbeat、Cancel、
  Replace、Timeout、Expired；另验证Created在权重读取时自动过期。
- `task_069_pending_record_and_refresh`：真实记录/统计Influence，负向源与正向目标，
  在读取时自动Timeout、延迟回收、零权重保持统计sum缺省。
- `task_069_zero_window_after_refresh`：推进时钟，在检测过期后才采样age，零窗口立即回收。
- `task_069_pause_resume`：IN/OUT两个方向，删除/新增/保留ID、缺失99返回类型化
  StoreNotFound、错误不阻断其他ID、重复集合无额外引用计数、恢复到空集。

无新增I/O；故障证据针对缺失store及真实操作取消/替换/超时/过期，不虚构存储故障。

[red-api](logs/task_069/red-api.log) 缺失模块/API，退出101。
首次locked执行要求登记statistics依赖，离线生成锁后仅保留路径依赖新增，撤销无关
pulldown-cmark升级。初次green编译还修正测试装配中的OP_LEADER为OpKind::LEADER。
[red-behavior](logs/task_069/red-behavior.log) 故意反转成功条件，使成功权重0、超时权重1，
同一测试实际2通过/2失败，退出101；恢复后聚焦5项通过。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_069 -- --nocapture` | 0 | 5通过，0失败，0忽略；37/37冻结案例通过及额外边界 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 21通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage废弃警告 |

每条确切argv、退出码和环境位于[logs/task_069](logs/task_069/)同名JSON及stdout/stderr日志。
全部Cargo调用通过run.py原子mkdir动态领取共享槽位，实际 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常/失败/中断等待子进程退出后释放自己锁，缓存保留；脚本仅清理本次新生成且
持有PID已不存在的临时etcd fixture二进制，遇权限问题保留，不触碰其他任务。

[audit](logs/task_069/audit.json) 核实1101个Go/模块文件的清单、SHA256与Git状态
不变，两份只读plan.md与HEAD一致。其他任务文件的既有删除不纳入本次提交。


最终 `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked`
退出 **0**，实际 **1570通过、0失败、1既有忽略**。
[最终完整日志](logs/task_069/workspace-final.log)、[计数](logs/task_069/counts.json)。
既有忽略是需要独立Go/Rust进程二进制的tso_processes；本任务没有新增忽略。
首次workspace回归已成功；补充状态刷新后时钟采样和零窗口测试后，重新运行最终
聚焦、包级、fmt/check及workspace验证，以workspace-final作为最终源码的证据。
所有自己的槽位锁已释放，缓存保留。日志只规范行尾空白和EOF空行，命令JSON记录
规范前后SHA256；业务fixture未归一化。`git diff --check`与暂存差异检查通过。

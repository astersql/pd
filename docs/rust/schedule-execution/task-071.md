# 任务071：领导者均衡配置更新恢复

状态：已完成。

依赖批次70的提交 `b6cad9b32` 已提供 BalanceLeaderConfig、真实 schedule、
核心状态/过滤器/placement/operator API。其24项包级及1573项全仓通过证据已读，
本任务包级验证再次覆盖这些接口。两份 plan.md 保持只读。

生产路径：[balance_leader_config.rs](../../../pkg/schedule/schedulers/balance_leader_config.rs)，
[API与边界](../../../pkg/schedule/schedulers/balance_leader_config.rs.md)。
BalanceLeaderScheduler 新增 `update_leader_config`、`reload_leader_config` 和
`encode_leader_config`，沿用唯一配置对象和真实 StorageEndpoint/MetaStore。
`LeaderConfigUpdate` 区分未变化与已更新，并保留持久化错误。
HTTP部分JSON更新、字段识别及状态码映射属于后续适配层，本任务提供已验证领域快照替换。

## 来源与语义

按用户后续授权使用**只读Go源码/测试推导 + Rust行为实测**。
没有本任务 Go 运行产物、Go/Rust 运行差分，也没有把 oracle-index 中其它运行结果
冒充本任务证据。[provenance](logs/task_071/provenance.json) 保存来源提交、文件及
fixture SHA256；Go binary/output hash 均为 null。

只读 `balance_leader.go` 的参数校验、clone、update、EncodeConfig、ReloadConfig、
Schedule 与相邻测试 TestBalanceLeaderSchedulerConfigClone、TestBalanceLeaderBatchLimit，
以及范围套件 MultiRangeBalance、BatchBalance 是期望依据。
TestMain仅是套件入口，没有当成行为覆盖。`config.go` 的 save/load 提供存储契约，
其中 persistFail 需要 failpoint 注入，因此未运行 Go 测试。

任务文字中的“持久化失败回滚”与真实 Go update 不同：非法 batch 回滚，
但合法更新先发布，save失败只记录警告并返回成功。本实现保留源码行为：
I/O错误可由 outcome 检查，活跃配置仍为新值，重载恢复旧持久化值。
这与通用 SchedulerConfig 的先持久化再发布语义不同，故没有套用那个 update API。
Reload使用批次70继承的已验证 BalanceLeaderConfig schema；损坏或非法batch的存储值
被拒绝并保留活跃值。Go直接反序列化的ReloadConfig不校验batch，故不声称对任意
损坏存储/缺失字段/空ranges重写的完全等价。完整有效配置的更新、编码、重载为本任务领域路径。

## 行为与失败验证

[fixture](fixtures/task_071.json) 手工冻结6例，不从待测实现生成期望：
全范围batch1/4/100（8个可用区域）、范围外、单区范围、多范围。
每例使用真实core region索引、store状态、规则管理器、labeler、统计快照及operator工厂。
先验证默认配置实际产生4条命令，再验证新配置与新实例reload后各自产生期望数量；
检查TransferLeader、source=1、目标属于{2,3}、唯一region及范围包含关系，pending influence不变。
显式随机/时钟及同分目标规则见provenance，无忽略错误。

2项测试：

- `task_071_behavior_cases`：6/6场景，更新/同值无操作、十六进制range编码、重建实例恢复、
  更新前后的实际决策与reload后决策一致；每例3轮真实schedule。
- `task_071_faults_and_validation`：batch0/101/usize::MAX拒绝并保留原值，clone独立；
  真实MemoryStore的I/O边界Unavailable注入，检查更新仍生效、存储仍为旧值、
  读失败保留新值、同值更新不依赖存储、恢复后reload回旧值；6种损坏/非法配置
  reload拒绝且状态不变；自定义名字的key隔离及恢复后新更新成功。

[red-api](logs/task_071/red-api.log)：先注册测试后缺失生产模块/API，退出101，
同时包含装配阶段的缺字段/方法错误，未把这些当作行为失败证据。
装配时还修复了显式provider缺少count策略的测试输入，未改变生产逻辑或冻结期望。
[red-behavior](logs/task_071/red-behavior.log)：故意移除发布候选配置，
同一两项测试分别在更新结果未变化和持久化失败后batch=4（期望100）断言失败，
0通过/2失败，退出101。随后恢复代码并重跑通过，证明测试识别真实行为回归。

## 验证

确切命令、退出码、槽位和输出在 [logs/task_071](logs/task_071/)。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_071 -- --nocapture` | 0 | 2通过、0失败/忽略，6/6固定场景 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 26通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，仅既有cpu_usage废弃警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1575通过、0失败、1既有忽略 |

全仓数量含实际执行的doc tests，零测试目标未用作行为验收。
既有忽略为需要独立Go/Rust进程二进制的tso_processes，没有新增忽略。
[计数](logs/task_071/counts.json) 从真实 test result 行求和。
每次Cargo阶段均由run.py原子mkdir动态领取共享槽位，实际均领取 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
每条命令结束均等待子进程退出后释放自己的锁，正常/失败/中断清理，保留缓存。
没有为任务新建Cargo缓存目录或清理其它任务数据。

[审计](logs/task_071/audit.json) 核对开始/结束1101个Go及模块文件SHA256一致，
Git状态为空，包含未跟踪文件扫描；两份只读plan.md无差异。
`git diff --check` 和提交前 staged diff 检查通过。
差异仅为本任务模块注册、领域API、测试、fixture和证据；没有改动Cargo依赖或其它生产模块。
工作区原有其它编号任务删除不纳入本任务提交。本任务完成后删除自身编号任务文件。
日志仅清理末尾空行以通过diff检查，保留全部命令输出内容。

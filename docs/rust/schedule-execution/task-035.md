# 任务035：操作限流与影响查询

生产路径 [controller_limit.rs](../../../pkg/schedule/operator/controller_limit.rs)，
接线 [lib.rs](../../../pkg/schedule/operator/lib.rs)，边界说明见
[controller_limit.rs.md](../../../pkg/schedule/operator/controller_limit.rs.md)。
只将 controller_admission 内部状态/两个辅助函数暴露给同 crate 的限流层，
没有复制 map/队列/计数，没有更改 Cargo.toml/Cargo.lock 或 Go 文件。

## 依赖、来源和证据性质

批次34提交 `066a6ba06` 的 [task-034.md](task-034.md) 记录8个聚焦、65个包级、
1428个 workspace 测试通过。已核实 ControllerAdmission 的准入/替换/移除、
waiting queue 和 primary-kind 计数 API，以及 Operator total/unfinished influence、
BasicCluster、StoreInfo 共享 limiter、SharedConfigProvider 速率读取接口。
这些是真实前置依赖，本任务继续以包级和 workspace 测试验证。

已读取 oracle-index.json。已有 operator_concurrent_replace 记录不能代表限流、
影响量或范围测试。按照用户后续授权，采用源码推导期望，不受缺少 Go 运行 oracle
准入限制；`source_assertion`/`go_execution=false`，未运行 Go，不声称运行差分。
[fixture](fixtures/task_035.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 及 Go 源码/测试 SHA256，验证脚本
逐字节核实该基准与当前来源。没有新增或改写 Go 工具/模块。

只读来源及实际采用的契约：

- `ExceedStoreLimit`：总成本按 store/type 聚合、首成员优先级、urgent 绕过、
  缺少 store 提前 false；Available 不消费 token。
- `addOperatorInner`/`ack`：成功准入后按成员 Take，移除/替换 Ack；v1 不退款，
  v2 归还 snapshot 成本。配置每分钟转每秒，仅重置 v1，Ack 也刷新当前配置；
  fixture 的7组成本场景分别验证 Ack 变更后速率为每秒2。
- `TestStoreLimit`：连续新增受预算约束、配置改变容量；并发入口
  `TestConcurrentAddOperatorAndSetStoreLimit` 设计100轮 reset 与200轮 Rust 新增/
  移除竞争，使用真实 limiter、OS 线程和宿主 runtime，无调用计数 mock。
- `TestInfluenceOpt`：nil cluster/option、缺少 region、原始范围比较。
- `TestCalcInfluence`：新增 learner 后剩余 leader/remove 影响与成本；
  `TestCacheInfluence`：首次总影响缓存，region 丢失后仍可复用。
- light 不产生 token/snapshot 费用但仍产生资源影响；fast cache 使用 total
  influence，按10秒 TTL、region ID 替换。分类与入缓存由后续 dispatch 宿主负责。

时间仅使用显式 monotonic clock/Instant 验证 timeout/TTL。token 测试使用真实 v1
限速器，每分钟0.000001的速率确保短测试不会重新填充200/1000 token；没有忽略
错误、状态、成本或资源字段。此行为没有持久化和外部 I/O 路径。

## 失败验证

[red.log](logs/task_035/red.log) 记录新增测试的缺失 API 编译失败（exit101）。
随后修正测试与既有 API 的接线，行为断言测试通过。为证明断言能识别错误行为，
临时将真实 limiter 的 Available 拒绝分支关闭，运行同一 `task_035` 过滤测试；
[mutation.log](logs/task_035/mutation.log) 保存实际断言失败（exit101），之后
finally 恢复正确实现并重新格式化、验证。该故障仅修改本任务 Rust 源码，未改 Go。

## 验证记录

每个命令经 [run.py](logs/task_035/run.py) 动态 mkdir 领取仓库共享槽位，记录
plan/task/session/PID，设置绝对 CARGO_TARGET_DIR；命令退出后删除自己的记录和锁，
保留缓存。每条命令/退出码/实际槽位保存同名 JSON，stdout/stderr 保存 log。日志只移除末尾多余空行以通过 diff 检查；
[log-normalization.json](logs/task_035/log-normalization.json) 记录移除换行数与原始
SHA256，验证脚本重建原始字节核对；错误、状态、测试与时间字段均未改写。
最终通过数量及 Go/plan 内容核对见下方最终结果。

## 最终结果

状态：已完成。最终源码与 fixture 的全部必需验证已获得新证据：

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_035 -- --nocapture` | 0 | 10通过，0失败，0忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 75通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1438通过，0失败，1既有忽略 |

聚焦10个测试覆盖7组 fixture 成本案例（7成功、0失败），另有9个命名行为故事与
200轮新增/移除竞争及100轮配置重设。故障注入关闭 Available 拒绝后5失败/5通过，
恢复正确实现后全部通过。workspace 唯一忽略来自既有测试；既有 server deprecated
field 警告不影响 check/test 退出码。不用编译成功或零测试替代行为验证。

本任务所有阶段实际动态领取槽位1，绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
全部 Cargo/测试子进程已退出，占用记录与锁目录均已释放，缓存保留。
[verification.json](logs/task_035/verification.json) 保存实际命令、退出码、有效测试
数量、源码哈希、Go/模块1101份文件哈希前后相同、Go Git 状态为空、两级只读
plan.md 未变、来源匹配与 Markdown 链接核对结果。fixture SHA256 为
`164fa721d56e1618d7506c8fdb47ef4e01f33567635383521a9d548190c77efc`。

未运行 Go/make/failpoint/生成/格式化入口，未改只读 plan.md。使用 git-commit
技能只提交本任务代码、独立测试和证据，排除其它任务已有删除。编号任务文件
在报告完成证据后删除，不将单独状态更新/删除纳入本任务提交。

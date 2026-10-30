# 任务049：Replication 配置热更新

状态：已完成。

基准 `c3f24896e`；依赖[任务048](task-048.md)已确认。Go/模块与 plan.md 只读。

## 实现与来源

[HTTP写入口](../../../pkg/server/http_replication_config_write.rs)接通 POST `/pd/api/v1/config/replicate` 与 `/pd/api/v1/config/label-property`，使用既有共享 listener、leader 转发与取消上下文。[宿主更新](../../../pkg/runtime/replication_config.rs)经既有协调器串行命令队列，在资源锁下构造候选、复用 ReplicationConfig 验证和 RuleManager 默认规则检查。配置与规则经同一 MetaStore 事务、TermStore leader fencing 提交，成功后无 await 地发布活动配置、规则列表和协调器配置。真实后端明确拒绝或提交前取消时不发布候选；后端结果不明确时不盲目重试。

为满足配置/规则原子持久化，扩展 canonical [RuleManager](../../../pkg/schedule/placement/rule_manager.rs)的 prepare/publish API，复用其规则校验、patch trim、版本与规则列表构建。宿主不复制规则算法。label-property 按 Go 的 set 去重、delete 删除空类型规则更新，持久化后同步调度 provider。

Go oracle：`server/api/config.go` 的 SetReplicationConfig/SetLabelPropertyConfig；`server/server.go` 的 SetReplicationConfig/SetLabelProperty/DeleteLabelProperty；`server/config/persist_options.go` 的 label-property 语义；`pkg/schedule/config/config.go` 的 ReplicationConfig Validate 与 `pkg/utils/typeutil/string_slice.go` 的字符串标签解码。

清单对应：[source-inventory](source-inventory.md)任务49行与 persist_schedule；[wire-inventory](wire-inventory.md) HTTP-028、HTTP-030；[surface-inventory](surface-inventory.md) ReplicationConfig 字段与验证、LabelProperty、router.go:178/180 注册点。早期矩阵把 label-property 归50，同时把 replication-mode 写入/状态归49；本编号任务明确限定 replicate 与 label-property，故仅更新这两个写入口证据，不宣称覆盖 HTTP-034/HTTP-142 或 DR 领域服务。

## 行为证据

[HTTP测试](../../../pkg/server/http_replication_config_write_test.rs)覆盖真实 TCP/etcd 写入、非法混合 patch 保留旧值、label set 去重/delete、实际持久文档和同身份重启恢复。[故障测试](../../../pkg/server/http_replication_config_write_fault_test.rs)通过真实 config 事务附加不存在 lease 触发 etcd 拒绝，比较配置与规则的完整存储条目/revision；验证 follower 转发、并发字段修改与取消后的 shutdown/join。[运行时消费者测试](../../../pkg/runtime/replication_config_test.rs)检查共享活动 RuleManager 的默认规则及调度 provider 的标签匹配结果。

[只读Go差分入口](../../../scripts/http-replication-config-oracle.py)复用任务004守卫与既有 Go 二进制。Rust运行时沿用既有 placement rules 默认关闭值，Go默认开启；bootstrap后向双方实际POST启用 placement rules 建立相同输入，不删除该字段或归一化配置。随后比较响应与完整 GET 配置，包含未bootstrap、成功、规则检查错误、JSON类型/语法错误、null字段、未知字段、开关及 label 操作。内部错误中的 Go 栈程序计数器依赖进程和二进制，仅将 stack 地址数组归一化为空数组；保留该字段及所有其他错误数据。顶层 replication JSON null 的 Go handler 会 nil 解引用，Negroni返回text/plain 500及PANIC首行；Rust直接返回同一错误首行/类型/状态，不触发panic或泄漏堆栈。差分保留Go原始输出，比较首行、元数据和配置不变，排除仅Go拥有的恢复堆栈。默认规则差分比较Rust持久文档与Go规则GET，排除创建时间及版本：两端初始化时间不同，持久文档按canonical契约保存调用者版本，而GET展示发布后版本；其余全部规则属性逐项相等。二进制 build info 不含 VCS revision，不声称它由当前 HEAD 构建。

首次失败验证：`cargo test --locked -p astersql-pd-server --lib task_049_ -- --nocapture` 编译成功、运行1个测试，退出101；POST尚无路由，Content-Type实际为空而断言要求JSON。实现期间的编译错误不计行为失败证据。

## 验证记录

动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；各阶段原子 mkdir 领取、记录计划/任务/会话/PID、结束释放自有锁并保留缓存。

所有Cargo命令使用默认features与 `RUST_TEST_THREADS=4`。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | server检查 |
| `cargo check --workspace --locked` | 0 | workspace检查 |
| `cargo test --locked -p astersql-pd-server --lib task_049_ -- --nocapture` | 0 | 2通过；1默认忽略差分另行运行 |
| `cargo test --locked -p astersql-pd-runtime --lib task_049_ -- --nocapture` | 0 | 1通过；共享活动规则、label provider及TiFlash关闭保护 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 173通过、0失败、10默认忽略，含integration target |
| `cargo test --workspace --locked --no-fail-fast` | 0 | 2064通过、0失败、47默认忽略 |

`PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-replication-config-oracle.py`退出0；内部命令 `cargo test --locked -p astersql-pd-server --lib task_049_differential -- --ignored --nocapture` 1通过、0失败、0忽略。30个replication输入与14个label-property输入，另含bootstrap前后准备与默认规则持久属性比较。最新原始报告：`target/rust-slot-1/http-replication-config-oracle-1793332497420875000.json`，`readonly_unchanged=true`、`process_group_clean=true`。

workspace全量通过后，复核修正了顶层数字/布尔/字符串错误的Offset并补充差分；最终代码重新通过fmt、server/workspace check、44个Go差分输入、聚焦及server全包测试。未重复整个workspace测试，不将较早的workspace运行描述为最后诊断修正后的运行。最终server全包再次173通过，所有默认忽略用例均单独列明，未将零测试当作行为证据。

开始/结束1101个Go与模块文件（含ignored/untracked）的存在性、SHA-256与Git状态完全一致；plan.md与HEAD字节一致。最终server回归期间1238个Rust/Cargo文件SHA-256不变。自有槽位锁已释放，编译缓存保留。Python语法、文档链接与git diff --check通过。

[压缩原始证据](task-049-evidence.json.gz)保存开始/结束守卫、最终源码哈希、全部迭代命令/输出/退出码、槽位runner、全部Go差分报告（含失败迭代与原始panic恢复栈）。SHA-256：`72d023e3050f76b60039069ba116798e420ebcc42b9e44f5d64f478d91006b3b`。

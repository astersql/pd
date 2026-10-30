# 任务050：服务配置热更新

状态：已完成（上述编号任务限定的服务字段与版本写入边界）。

基准 `27586d1d6`；依赖[任务049](task-049.md)已确认。Go 与模块文件、plan.md 全程只读。

## 实现及边界

[HTTP适配](../../../pkg/server/http_server_config_write.rs)接通共享 listener 的 POST `/pd/api/v1/config` 与 `/pd/api/v1/config/cluster-version`，沿用 leader 转发与请求取消。顶层支持 pd-server 字段的短名和点分前缀、cluster-version，并转交已有 schedule/replication 更新入口；schedule 字段合并提交及重复字段报错保持 Go 语义。pd-server 不新增独立 POST 路由（Go 只有对应 GET）。顶层多字段保留 Go 逐项写入语义，并非跨领域原子事务；Go map 遍历无顺序保证，Rust 使用稳定键序。

[运行时](../../../pkg/runtime/server_config.rs)使用既有协调器串行队列和资源锁，从最新配置创建候选，复用 canonical PDServerConfig 验证及 ClusterVersion 类型。dashboard-address 须为 auto/none 或真实注册成员 client URL。配置经 TermStore leader fencing 的真实事务持久化，成功后无 await 地发布活动文档、调度 provider、调度器配置与 resolved-ts worker 间隔；版本门禁直接读取活动文档。后端明确失败或提交前取消不发布候选；结果不明确的后端错误不盲目重试。

按编号任务限定，本次验收服务配置字段及版本写入，不声称整个顶层 endpoint 的所有领域已完成：replication-mode、keyspace、micro-service、controller 仍依赖各自领域服务；TTL 配置尚无运行时 lease/到期消费者，明确返回 unavailable，不能把临时配置静默写成永久配置。log 动态重配置未接通。Go 专属 GC tuner、Dashboard 进程、region-storage 后端切换不由本任务创建；其配置可持久化，不宣称等价实现这些运行时。主题8依赖保持原归属。

## 来源映射

- [source-inventory](source-inventory.md)：任务50、server/api/config.go、server/config/persist_schedule.rs。
- [wire-inventory](wire-inventory.md)：HTTP-022 的本任务字段写入子集、HTTP-032；HTTP-030 label-property 已由任务049覆盖，不重复实现。未把上述依赖的 unavailable 当成 Go 差分通过。
- [surface-inventory](surface-inventory.md)：PDServerConfig C-195–C-209 字段/验证、ClusterVersion 与 router.go:172/182；默认值与读取复用任务047。
- Go oracle：`server/api/config.go::SetConfig/updateConfig/updatePDServerConfig/updateClusterVersion/SetClusterVersion`，`server/server.go::SetPDServerConfig/SetClusterVersion`，`pkg/utils/jsonutil/jsonutil.go`，`pkg/versioninfo/versioninfo.go::ParseVersion`。时长错误复用已有 HTTP schedule 适配帮助函数。

## 行为证据

[HTTP测试](../../../pkg/server/http_server_config_write_test.rs)覆盖实际 TCP/etcd 写入、非法值保留旧状态、成员 dashboard 地址检查、持久文档及同身份重启恢复。[故障测试](../../../pkg/server/http_server_config_write_fault_test.rs)复用真实 config 事务故障工具：附加不存在 lease，让 etcd 实际拒绝；比较完整存储条目及 revision，验证 pd-server 和 cluster-version 均不发布失败候选；另测 follower 转发、并发不同字段更新、事务前暂停后取消及 shutdown/join。[消费者测试](../../../pkg/runtime/server_config_test.rs)验证共享调度 provider 的 key-type/flow-round、resolved-ts 间隔和 batch split 版本门禁已改变，非法变更不会发布。

首次红灯：`cargo test --locked -p astersql-pd-server --lib task_050_ -- --nocapture` 编译成功，1个行为测试失败，退出101。POST缺失导致Content-Type为空，而真实HTTP断言要求JSON；不是编译错误证据。

[只读差分入口](../../../scripts/http-server-config-oracle.py)复用任务004守卫及已有 Go 二进制；67个输入逐项比较响应状态、JSON和更新后的完整 pd-server/cluster-version GET。包括全部15个PD server字段的写入或边界、短名/点分、重复schedule字段、整数浮点JSON表示、布尔字符串、null、数组、未知字段、范围/时长/版本错误。仅归一化 Go 错误 stack 内进程相关地址，保留 stack 字段和其余错误内容。先对两端bootstrap并实际启用placement rules，消除原有默认配置差异。Go二进制build info没有VCS revision，不声称来自当前HEAD。原始报告 `target/rust-slot-1/http-server-config-oracle-1793333208720483000.json`：退出0，1通过，readonly_unchanged/process_group_clean均true。

## 验证

所有 Cargo 进程使用默认 features、`RUST_TEST_THREADS=4`，动态原子领取槽位1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。每阶段记录任务/计划/会话/PID，结束释放自己的锁，保留编译缓存。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | server检查 |
| `cargo check --workspace --locked` | 0 | workspace检查 |
| `cargo test --locked -p astersql-pd-server --lib task_050_ -- --nocapture` | 0 | 2通过、1默认忽略差分另行运行 |
| `cargo test --locked -p astersql-pd-runtime --lib task_050_ -- --nocapture` | 0 | 1通过 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-server-config-oracle.py` | 0 | 内部 `cargo test --locked -p astersql-pd-server --lib task_050_differential -- --ignored --nocapture` 1通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 175通过、0失败、11默认忽略，含integration target |
| `cargo test --workspace --locked` | 0 | 2067通过、0失败、48默认忽略 |

开始/结束1101个Go与模块文件（含ignored/untracked）的存在性、SHA-256及Git状态一致；plan.md与HEAD字节一致。最终全量验证期间1243个Rust/Cargo文件哈希未变。自己的槽位锁已释放，缓存保留；Python语法、文档链接与git diff --check通过。默认忽略用例不计为通过，本任务差分已单独实际运行。

[压缩原始证据](task-050-evidence.json.gz)保存完整守卫、源码哈希、命令/输出/退出码、槽位runner及全部差分迭代报告。SHA-256：`34ab62c46d8e114620523d25dc0b24ddf02b6d65c6a7f8ce7660417fdc527f8b`。

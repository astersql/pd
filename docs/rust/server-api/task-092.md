# 任务092：日志初始化与热级别

状态：已完成。

基准：`1d541a376`。任务91已有当前CLI、真实进程及只读oracle证据；已读取
plan.md、前三项盘点清单、任务4入口和91证据。总计划及Go/模块只读。

## 实现与范围

独立`main`初始化tracing订阅器，支持JSON及text/console、stdout或追加文件、
禁用时间戳/调用位置；生命周期和失败使用结构事件。进程只接收显式`pd` target事件，
当前事件字段仅含允许的级别字符串，不输出完整配置、请求、URL或后端错误。
初始化及后端失败只输出稳定分类，端口占用保留既有CLI错误分类。
配置解析失败也只输出稳定分类，保留两条不含输入值的既有验证提示；
修复TOML错误在日志初始化前直接回显原始配置片段的问题，保持既有退出码和输出通道。

`LogLevelControl`为宿主拥有的接口，`PdNodeBuilder::with_log_level_control`
沿现有RoleServices挂接`POST /pd/api/v1/admin/log`。
库不会初始化/替换全局订阅器；未提供控制器的嵌入宿主返回503。
热更新只修改原子阈值，不能更换writer、格式或脱敏策略；非法输入保留原级别。
采用动态过滤器，避免静态callsite缓存使DEBUG在热更新后仍不可见。
`debug/info/warn/warning/error/fatal`大小写不敏感；fatal禁用所有非fatal事件，
没有用Rust的ERROR事件冒充Go fatal或引入进程退出副作用。

主题9的日志轮转、采样及运维功能仍非本次接线验收范围；没有声称所有LogConfig
字段与zap完全等价。Go embedded-etcd与Rust外部MetaStore的拓扑差异保持原边界。

## 来源与证据映射

- [source-inventory.md](source-inventory.md)：任务92与日志/入口共享来源行；
  `pkg/utils/logutil/log.go`的合法级别、初始化、脱敏及`server/server.go:SetLogLevel`。
- [wire-inventory.md](wire-inventory.md)：HTTP-141，classic POST admin/log；
  已存在的本地路由例外沿用，无需leader或Raft bootstrap。
- [surface-inventory.md](surface-inventory.md)：日志值、CLI log-level/log-file以及
  90/91/92共享准入/manifest行。完整相关原行号和原文归档至压缩证据；
  共享manifest仅证明来源，不把无关配置/指标计算为本次功能通过。
- 生产路径：[cli_logging.rs](../../../cmd/pd-server/cli_logging.rs)、
  [main.rs](../../../cmd/pd-server/main.rs)、[http_log.rs](../../../pkg/server/http_log.rs)、
  [node.rs](../../../pkg/runtime/node.rs)、[member_role_worker.rs](../../../pkg/runtime/member_role_worker.rs)。
- 独立测试：[cli_logging_test.rs](../../../cmd/pd-server/cli_logging_test.rs)，lib显式注册。
  验证实际同一日志点info→debug→非法级别→fatal→warning过滤；JSON结构、依赖敏感事件隔离；
  真实TCP成功/错误、同一MetaStore上的第二成员follower本地修改（无重定向）、
  嵌入宿主策略和持久配置值不变；100 Continue确认请求体正在读取后取消，
  返回500且原级别不变，shutdown后端口释放，宿主订阅器仍可输出。

## Go差分

[cli-logging-oracle.py](../../../scripts/cli-logging-oracle.py)复用任务4只读守卫和已有
Go二进制、slot内Rust二进制与etcd fixture；不生成或改写Go，不使用failpoint入口。
两个真实进程分别由HTTP members确认就绪，比较17组admin/log状态码及JSON响应
（合法大小写/别名、trace、空字符串、null、错误类型、畸形JSON、空body、超过1MiB的非法级别），全部一致。
两个JSON日志文件均观察到6次实际允许的级别变更事件；fatal/error阈值抑制warning。
SIGTERM退出0，端口可重绑，子进程组均结束。Rust另以含合成凭据的后端地址启动失败，
退出1，stdout/stderr均无合成秘密；另以含合成秘密的畸形TOML启动，
退出0且原文不再出现在stdout/stderr，保持Go兼容的解析失败退出约定。日志字段布局不作字节等价承诺。
Go二进制无VCS revision，按文件SHA-256固定，不能推断来自当前HEAD。

## 验证记录

首次真实TCP失败：可编译测试返回404而不是嵌入无控制器应有的503，退出101，
0通过/1失败。后续静态过滤器版本有实际断言失败，修复后同一日志点热更新通过。
包回归曾发现端口错误分类大小写变化，已保留既有约定并重跑。
后续真实进程断言发现TOML错误原文泄露合成秘密，退出1；修复后由独立子进程测试及真实二进制重验。
离线首次依赖解析因缺tracing-serde缓存失败，之后取得锁定依赖并全程使用`--locked`验证。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-cmd-pd-server --lib task_092_ -- --nocapture` | 0 | 4通过，0忽略 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --nocapture` | 0 | 16通过（15 lib + 1 TLS），0忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 真实进程产物 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_CLI_LOGGING_REPORT=/tmp/task092-oracle.json python3 scripts/cli-logging-oracle.py` | 0 | 17组真实HTTP差分、文件日志和退出清理 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo check --locked -p astersql-pd-cmd-pd-server` | 0 | 包级编译 |
| `cargo check --workspace --locked` | 0 | workspace编译 |

| `RUST_TEST_THREADS=1 cargo test --workspace --locked`（完整回归检查点） | 0 | 2209通过，76项既有忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_CLI_CONFIG_REPORT=/tmp/task092-config-oracle.json python3 scripts/cli-config-oracle.py` | 0 | 9组配置差分及真实HTTP/停机 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_CLI_EXIT_REPORT=/tmp/task092-exit-oracle.json python3 scripts/cli-exit-oracle.py` | 0 | 18组CLI、端口冲突与四种信号差分 |

完整workspace测试检查点在最后的配置错误脱敏及请求体读取边界修正之前。
最终源码已再次运行上表的4项聚焦、16项包级测试、fmt、metadata、包级及workspace check、
真实日志进程差分和相邻CLI/配置差分；没有把检查点当作最后两处修正后的全量重跑。
超过1MiB的非法级别仍按Go契约返回400，未引入日志API专用的额外请求体上限。

## 槽位与只读守卫

所有Cargo及产物使用阶段通过原子mkdir领取共享槽位。主要及最终验证使用slot-1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
一次中间失败验证使用slot-2：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
默认features，`CARGO_BUILD_JOBS=4`；完整workspace与最终包级测试使用`RUST_TEST_THREADS=1`。
阶段退出trap删除自己的owner和锁；所有自有进程组与锁均清理，两个槽位缓存保留。

完整原始输出、确切命令/退出码/测试数、二进制与最终源码哈希、Go前后守卫及来源行归档，见
[压缩证据](task-092-evidence.json.gz)。Go/模块保护集合1101文件（含ignored/untracked），
开始/结束路径集合、每文件SHA-256及Git状态完全相同；plan.md字节与基准一致。
Python AST、文档链接及Git diff检查通过。仅提交本任务变更，保留用户原有prompt.md修改。

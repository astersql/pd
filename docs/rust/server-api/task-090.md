# 任务090：配置文件与 CLI 合并

状态：已完成。基准 `7b51e0add`；2026-11-01。批次89证据及任务1–4的三份清单、只读oracle入口已核验。固定协议不变。

## 合并和接线

[CLI适配](../../../cmd/pd-server/cli_config.rs)通过现有 `decode_toml_config`、
`adjust_deprecated_from_file`、`Config::adjust` 完成文件解码→废弃字段迁移→CLI覆盖→默认调整，
保留完整配置和元数据，不复制领域默认值。支持等号、独立字符串参数和 `-L`；合法重复参数最后一次生效。字符串参数可由 `-` 开头；
每次布尔参数都立即校验，非法值不能被后面的合法重复值掩盖。
Go `AdjustCommandLineString` 实际忽略空字符串，`AdjustCommandLineBool` 只以true覆盖，
所以空CLI不清除文件值、false不覆盖文件true。默认名称使用实际主机名，数据目录使用实际绝对cwd；
没有凭空增加 `PD_NAME` 等环境变量绑定。Go唯一相关 `PD_SERVICE_MODE=api` 是服务模式选择，
不参与这些字段的覆盖，独立MCS模式仍属后续命令/主题8边界。

[进程入口](../../../cmd/pd-server/main.rs)将合并后的身份/地址接入bootstrap，将完整配置交给已有
listener/telemetry builder；TLS与指标地址保留在canonical配置中。`--config-check` 输出与Go相同的
成功文本或文件警告，在创建executor、监听器、连接MetaStore之前退出。
日志配置值的合并、废弃迁移与路径校验在此完成；安装宿主logger属于任务92。
version/help的完整契约、错误退出码对齐属于任务91；本任务没有将Go解析错误返回0与Rust返回2说成一致。

显式配置的 `data-dir`、`initial-cluster`、`force-new-cluster` 在校验模式保留实际值并运行Go式校验，
在外部MetaStore实际启动前明确拒绝，错误为 `external MetaStore backend cannot apply: ...`。
默认推导出的data-dir/initial-cluster不触发拒绝。拒绝先于任何listener/存储操作，不能再作为无意义元数据静默忽略。
这保留主题10的embedded-etcd/本地持久化边界，不声称实现Go内嵌拓扑。
[兼容harness](../../../tests/rust-compat/harness.rs)只给Go传嵌入式etcd参数，Rust继续使用真实外部etcd；
其余参数仍在基础参数后覆盖，未改变既有extra优先级。

## 行为证据和差分

[独立测试](../../../cmd/pd-server/cli_config_test.rs)在lib.rs显式注册：3项真实测试覆盖文件与CLI优先级、
空值保留、主机默认名称、绝对默认目录、广告地址回退、deprecated新旧日志优先级及警告、TLS/metrics覆盖、
true文件与false CLI、文件缺失/非法TOML/未知flag/缺参数/错误bool/join冲突。
真实etcd与HTTP启动测试核对文件配置的成员名称和地址；正常shutdown后端口可重绑、成员状态实际持久化；
不支持的data-dir启动失败后完整 `/pd/` 条目不变，也没有创建本地目录。fixture复用已有注册，
避免同一测试进程内复制fixture导致目录计数器冲突。

[只读Go/Rust进程入口](../../../scripts/cli-config-oracle.py)运行9组配置校验差分：默认、文件/CLI、
废弃字段、日志子目录拒绝、CLI路径修复、空CLI保留错误、join冲突、true文件/false CLI和非法TOML。
每个场景保留两个进程的原始argv/输出/退出码，以成功文本、实际警告或错误类别判定；非法TOML两者均拒绝，
不伪造相同错误文本或退出码。环境显式提供 `PD_NAME`，实际启动仍由文件/CLI名称决定。
另用同一入口分别启动真实Go和Rust：配置文件名称from-file被CLI名称from-cli覆盖，client/peer地址来自文件；
真实HTTP `/pd/api/v1/members` 核对名称和两类地址。两进程SIGTERM均退出0、端口释放。
Go使用独立临时embedded-etcd目录，Rust使用本槽位已构建fixture的真实外部MetaStore；
不把二者存储拓扑当成同一实现。既有health/member双实现入口额外核对HTTP和GetMembers RPC并清理进程。

Go oracle为已有 `bin/pd-server`，SHA-256
`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；
go1.26.8、without_dashboard、darwin/amd64，无VCS revision，不能声称由当前HEAD构建。
fixture复用当前包级测试按只读规则构建的二进制，不生成或改写Go源码。
原始输出、二进制摘要、完整只读快照、来源行归档见[压缩证据](task-090-evidence.json.gz)。

## 来源映射

[source-inventory](source-inventory.md)任务90映射、main.go/main.rs、server/config及共享源码/manifest行，
[surface-inventory](surface-inventory.md)全部显式owner90行（包含C-138–143、C-147–169、C-171–180、
C-182–189、C-191–220、C-223–239及F-045–062和默认/Adjust附录）按原行号、原文、清单SHA-256归档。
[wire-inventory](wire-inventory.md)无独立owner90行；公共HTTP/RPC准入由真实health/member回归覆盖。
配置字段继续调用原有完整canonical decoder/adjust；原配置库的标量、嵌套、废弃、持久化测试参与workspace回归，
不是重新实现所有来源包。TLS复用83–85，指标复用86；F-045版本与完整命令展示交给91，日志安装交给92，
Go内嵌拓扑与独立MCS边界按上文保留。此证据覆盖文件/CLI合并边界，不宣称所有配置字段的服务热更新均由本任务完成。

## 验证记录

所有Cargo阶段从1–10原子mkdir动态领取，实际均为slot-1；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁记录task/session/持有shell PID，以EXIT trap释放自己的owner和锁目录，缓存保留。
默认features，`CARGO_BUILD_JOBS=4`；通常 `RUST_TEST_THREADS=4`，最后完整workspace回归使用1。
Rust修改后先 `cargo fmt --all` 再运行验证。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-cmd-pd-server --lib task_090_ -- --nocapture`（首次） | 101 | 0通过、1失败；可编译运行，断言文件必须合并，实际unsupported --config |
| 同一聚焦命令（最终） | 0 | 3通过、0忽略 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --nocapture` | 0 | 9通过、0忽略 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo check --locked -p astersql-pd-cmd-pd-server` | 0 | 编译检查 |
| `cargo check --workspace --locked` | 0 | 编译检查 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_CLI_CONFIG_REPORT=/tmp/task090-oracle.json python3 scripts/cli-config-oracle.py` | 0 | 9组真实校验差分、1组真实HTTP身份/停机差分 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server PD_COMPAT_MODE=dual bash scripts/run-pd-compat-readonly.sh --case health-member` | 0 | 1通过、0忽略；HTTP/GetMembers RPC |
| `cargo test --workspace --locked`（4线程首次） | 101 | 既有TLS端口重绑AddrInUse；runtime 70通过、1失败、36忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_083_invalid_tls_releases_listener_without_storage_effects -- --nocapture` | 0 | 1通过、0忽略 |
| `RUST_TEST_THREADS=1 cargo test --workspace --locked`（最终） | 0 | 2203通过、76项既有忽略 |

早期包级验证发现新增测试重复包含fixture模块，引起相同目录路径碰撞（6通过、2失败），修复为共享原fixture后9项通过。
新增宿主uname依赖只在Cargo.lock记录已有rustix边；首次locked检查明确拒绝更新锁，随后
`cargo check --offline -p astersql-pd-cmd-pd-server`退出0更新该一行依赖边，最终所有验证均locked。

最终Go只读守卫：开始/结束1101个Go及模块文件（含ignored/untracked）的路径、内容SHA-256和Git状态完全一致。plan.md字节与基准HEAD一致。全部测试及oracle/fixture进程已退出，自己的owner和slot-1锁目录已删除，缓存保留。Python AST、证据文档源码链接及git diff --check通过。用户原有prompt.md变更保留，不纳入本次提交。

完整串行回归后，再次对最终参数边界运行聚焦3项、包级9项、编译/格式检查、9组校验差分与真实HTTP/停机差分、health/member双实现回归，均退出0。

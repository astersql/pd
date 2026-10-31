# 任务091：帮助、版本与进程退出码

状态：已完成。基准 `b43e510cf`；批次90证据及任务1–4的清单和只读oracle入口已核验。

## 行为与边界

[cli_exit.rs](../../../cmd/pd-server/cli_exit.rs)在创建runtime、listener和MetaStore之前处理参数语法、
help和version；参数表与[cli_config.rs](../../../cmd/pd-server/cli_config.rs)共享。
[main.rs](../../../cmd/pd-server/main.rs)继续复用canonical配置解码和校验。
help支持`--help`、`-h`和`help`；version支持`--version`、`-V`及显式布尔值，重复值最后一次生效。
完整扫描参数语法后才处理help；help跳过配置文件读取，version仍先校验配置，与实际Go进程一致。
版本显示实际Cargo包版本，构建元数据取可选`PD_GIT_HASH`、`PD_GIT_BRANCH`、`PD_BUILD_TIME`，
未提供时明确为unknown；保留Go六个版本字段标签，另标明Rust实现，不伪造Go二进制的版本。

| 场景 | stdout | stderr | 退出码 |
| --- | --- | --- | --- |
| help/version | 帮助或版本 | 空 | 0 |
| 未知参数、缺字符串参数、非法布尔值 | 诊断和usage | 空 | 1 |
| 配置文件缺失、配置校验失败 | 配置错误 | 空 | 0 |
| config-check成功 | `config check successful`或真实文件警告 | 空 | 0 |
| 实际启动失败（如端口占用） | 启动错误 | 空 | 1 |
| 缺少Rust外部etcd参数 | 空 | 外部依赖诊断 | 1 |
| SIGTERM（已就绪） | 生命周期输出 | 空 | 0 |
| SIGINT/SIGHUP/SIGQUIT（已就绪） | 生命周期输出及中断诊断 | 空 | 1 |

保留Go现有“配置错误退出0”的实际契约。帮助列出全部当前classic参数和Rust诊断参数，
明确必须提供运行中的外部etcd，help/version/config-check不连接etcd；没有宣称支持embedded-etcd。
Go专有的MCS services、completion命令不在本classic入口中展示为可用功能。
Go默认启动错误写stdout；Rust在同一进程边界输出错误，日志格式和日志宿主安装继续归任务92。
缺少外部etcd是Rust过渡模式额外的启动错误，Go没有对应参数，不作为等价差分场景。

## 测试与差分

[独立测试](../../../cmd/pd-server/cli_exit_test.rs)在lib.rs显式注册。
两个行为测试通过新子进程调用同一个真实CLI入口；额外一个测试仅作为子进程入口，
不把它算作独立行为验收。只剥离Rust测试harness固定的“running 1 test”前缀；
子进程捕获原stdout/stderr和退出码，不依赖旧二进制或在测试中嵌套Cargo。
边界覆盖版本/帮助别名、true/false/重复值、help与非法flag、help跳过配置、
version先校验配置、非法bool、缺参数、配置冲突、无外部etcd和真实TCP端口占用。

[只读进程差分](../../../scripts/cli-exit-oracle.py)使用已有Go二进制和当前槽位构建的Rust二进制，
逐进程保存argv、两个输出通道和退出码。共享任务90的真实etcd/HTTP生命周期入口，
增加可选信号参数，原默认SIGTERM调用保持原行为。真实HTTP members核对合并后的名称及client/peer地址，
分别发送SIGTERM/SIGINT/SIGHUP/SIGQUIT，核对退出码、监听端口可重新绑定、服务和fixture进程组均结束。
启动失败占用真实TCP端口；Go使用独立临时embedded-etcd目录，Rust使用外部MetaStore参数，
均在正常服务就绪前失败。成功启动使用真实etcd持久成员身份，不把CLI信息输出当作网络/存储验收。
共享health/member入口另核对HTTP/GetMembers RPC。外部与内嵌etcd拓扑仍不等价。

只比较语义、输出通道和退出码；usage排版、实现专属说明、版本值、错误文本及日志格式保留原始输出，
不声称字节一致。Go oracle为已有`bin/pd-server`，其构建信息无VCS revision，不能证明来自当前HEAD；
固定二进制SHA-256和原始观察保存在压缩证据中。

## 来源映射

[source-inventory.md](source-inventory.md)的任务91行、main.go/main.rs和共享源码/manifest owner91行；
[surface-inventory.md](surface-inventory.md)的F-045–062（版本、配置、配置校验及classic CLI参数）和
全部共享90/91/92 manifest/准入行，在证据中按原行号和原文归档。
参数值合并、默认值与废弃字段沿用任务90；本任务补展示、提前退出和失败分类，不重新实现领域配置。
[wire-inventory.md](wire-inventory.md)没有独立owner91行，公共RPC/HTTP准入通过真实health/member回归验证。
共享manifest行仅是来源追踪，不能把它们计作已逐项运行的功能。

## 验证记录

最终验证阶段的Cargo和相关子进程使用动态原子mkdir领取的共享slot-1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
默认features，`CARGO_BUILD_JOBS=4`，完整回归`RUST_TEST_THREADS=1`；保留缓存。


| 确切命令 / 阶段 | 退出码 | 有效测试或观察 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-cmd-pd-server --lib task_091_ -- --nocapture`（首次） | 101 | 可编译运行；0通过、2断言失败；version实际退出2，语法错误也退出2 |
| 同一聚焦命令（最终） | 0 | 3通过、0忽略；2个行为测试和1个子进程分发入口 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --nocapture` | 0 | 12通过、0忽略；包含上述分发入口 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo check --locked -p astersql-pd-cmd-pd-server` | 0 | 包级编译 |
| `cargo check --workspace --locked` | 0 | workspace编译 |
| `RUST_TEST_THREADS=1 cargo test --workspace --locked` | 0 | 2206通过、76项既有忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 当前真实进程产物 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_CLI_EXIT_REPORT=/tmp/task091-oracle-final.json python3 scripts/cli-exit-oracle.py` | 0 | 18组CLI、1组启动失败、4组真实HTTP/信号差分 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server PD_COMPAT_MODE=dual bash scripts/run-pd-compat-readonly.sh --case health-member` | 0 | HTTP/GetMembers RPC，1通过、0忽略 |

首轮真实端口冲突观察到Go错误在stdout、Rust在stderr，随后修复并重新运行真实二进制差分。
所有四种信号均通过真实Go/Rust进程验证；每个进程就绪后由HTTP确认身份，退出后端口可重绑，
服务与fixture进程组均清理。包级既有task090真实存储测试继续检查持久成员数据及失败无写入。
最终CLI变动后补跑聚焦、包级、格式及workspace编译检查，避免只依赖先前编译的测试产物。

初期锁持有shell因stdin关闭提前退出，之后重新领取并将最终验证放入同一持有shell，
记录session/PID，以EXIT trap清理自己的owner及锁目录；初期失败断言保留为真实观察，
不把该锁脚本问题算作产品行为失败。实际所有构建目录均为上述slot-1，没有创建任务专用target。

原始失败/通过输出、Go/Rust二进制SHA-256、18组进程输出、四种信号结果、
完整只读守卫与来源行归档见[压缩证据](task-091-evidence.json.gz)。Go二进制SHA-256：
`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。
最终Go保护集合1101个文件（含ignored/untracked），存在性、每文件SHA-256与Git状态均保持一致；
plan.md字节与基准一致。全部自有进程和槽位锁已清理，缓存保留。Python AST、文档链接和git diff检查通过。
用户原有prompt.md变更不纳入提交。

# 第 18 项本地原生拓扑验收

**本地验收完成。** 本轮接线基准为 `95c8360d3e47e879f9d9322380a4d668a01168cc`。
按用户授权，主题 9 的整体准入独立于此次本地入口。生产认证、成员管理、
切换、主题 9 及实际发布继续采用原门槛。责任来源为
[服务端 102](../task-102.md#L40)及[分项交接](handoff.md#L39)。
[用法和限制](native-topology.md)、[原始回执](task-18.json.gz)。

## 实际行为

- 正式 `pd-server` 默认连接外部 etcd。成对指定本地参数后，直接打开现有
  SQLite/OpenRaft 单 voter 后端，沿用相同的 classic 装配和 `PdNodeBuilder`。
  本地进程清空环境、PATH 指向不存在的目录；开始及 WAL 重开后服务期间采样
  均无子进程。CLI 拒绝半套参数、零共识 ID、etcd 和 join 选项、外部地址、
  API mode、目录竞争、错误共识 ID 及包含其他文件的目录。
- 正式 release 通过真实 TCP GetMembers、AllocID、Tso、HTTP health。
  真实 MetaStorage Put 确认后立即读取原字节。SIGKILL 退出信号 9、WAL
  留存且监听关闭。同一目录、成员和地址重开后读回相同 PD cluster ID 及
  已确认字节，ID 和 TSO 均前进。SIGTERM 退出 0，日志先出现 `PD stopped`
  再出现 `local native store closed`。测试随后重绑地址并重新独占打开后端。
- 同一套核心请求在固定 Go oracle 的独立夹具上运行。比较首次分配 ID、成功
  响应和健康状态，各自的随机 cluster ID、时间戳与端口原样保留。此轮 Go
  对照使用自己的 embedded etcd。外部 etcd 默认路径也用同一个 Rust release
  另行验证，健康、就绪、版本和 SIGTERM 端口清理均通过。
- 嵌入宿主复跑真实信号与 SQLite WAL 场景。调用方自行处理 SIGUSR1、持有
  Tokio 和日志，服务前后无额外子进程。SIGKILL 后复用 WAL，ID 和 TSO 前进。
  SIGTERM 后宿主显式关停存储，调用方执行器仍可使用，地址可重绑。

## 验证命令

| 命令 | 结果 |
| --- | --- |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 退出 0 |
| `cargo build --release --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 退出 0 |
| `PD_NATIVE_RELEASE_BIN="$CARGO_TARGET_DIR/release/pd-server" PD_NATIVE_GO_ORACLE="$PD_COMPAT_GO_BIN" cargo test --locked -p astersql-pd-cmd-pd-server --test native_local_cli -- --nocapture` | 退出 0，2 通过、0 失败、0 忽略 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --test-threads=1 --nocapture` | 退出 0，24 通过、0 失败、2 默认忽略；包括嵌入拓扑、既有本地迁移回滚、外部模式和 TLS |
| `python3 docs/rust/ops-tools/check-minimal-release.py "$CARGO_TARGET_DIR/release/pd-server" "$PD_COMPAT_GO_BIN" "$PD_COMPAT_ETCD_BIN"` | 退出 0，Rust 外部 etcd 与 Go 两场景通过 |
| `cargo metadata --format-version 1 --no-deps`、`cargo fmt --all -- --check`、`cargo check --workspace --locked`、`git diff --check` | 均退出 0，完整 workspace 测试沿用第 24 项的验收归属 |

旧 release 实际拒绝新参数，退出 1。接线后的第一次聚焦执行中，WAL 行为通过，
配置诊断测试因固定错误语句未列入脱敏清单而失败。随后固定文案纳入清单。
第一次 release 对照中，Rust 场景成功，Go oracle 因 Darwin 需要 `vm_stat`
而提前退出。仅为 Go 对照补 `/usr/bin:/bin`，Rust 本地进程仍使用空工具路径。
原始回执保留这些失败和最终结果。

本轮实际槽位 1，绝对 `CARGO_TARGET_DIR` 为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`。Go PD、kvproto ZIP、protoc
和官方 etcd 使用 [冻结输入](../../ops-tools/task-132.md#复现)。所有子进程退出后
释放槽位。Go 源码和模块路径共 1101 项首尾快照一致，SHA-256 为
`22045dc3e33a47a167e35d8a4565bc3337dedb1e47a845e98967087b9e8899ed`。

生产端继续复用 [本地迁移回滚](../../production-meta/task-38.md)和
[同 etcd Go/Rust/Go 回执](../../production-meta/task-39.md)。
跨节点认证及成员管理、正式生产切换回滚、主题 9 完整交接及生产发布见
[生产 ADR](../../production-meta/adr.md)和[运维 134](../../ops-tools/acceptance.md)。

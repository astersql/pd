# 第 22 项 Go 客户端与完整 Classic PD 回归

已完成。运行基准 `026616d053eaaedb3fd51db5e5ee5ba8c2f3beea`。
[批次 21](task-21.md)交接配置与构建准入，[批次 20](task-20.md)交接固定协议注册。
已核对批次 3–8 的 GC 对外接线和真实差分入口。两项废弃 watch 的原断言保持不变。
[矩阵](rpc-regression.md)逐组列出实际执行的方法和回执键，
[机器回执](task-22.json.gz)保留本轮请求观测、持久状态、来源摘要、命令和原始日志。

初始目录断言实际执行并失败，列出了旧 41 项目录缺少的 15 个方法。
使用现有六组双进程 GC 矩阵覆盖这些方法，监督器逐组精确选择测试，
保留原场景生成的 RPC 及 etcd 记录。首次联跑还发现监督器登记的三个诊断文件
已经从 `pkg/server` 移至 `pkg/mcs/server`，更新对应路径后再跑。
机器回执保留该轮失败，目录通过与实际场景通过分别计数。

Go 客户端的机器回执键为 `compat-readonly-1794152953670136000.json`。回执归档 SHA-256 为
`a6ad44813fb1f9bbe01c87e3fe43a8269bd40714b18c9629cafb380b3688e56e`。

## 命令和有效测试

| 命令 | 退出码 | 通过 / 失败 / 忽略 |
| --- | --- | --- |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `cargo test --locked -p astersql-pd-compat-tests --lib sa_022_ -- --nocapture`（目录红灯） | 101 | 0 / 1 / 0 |
| 同一 `sa_022_` 命令（最终） | 0 | 2 / 0 / 0 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_099_ -- --nocapture`（只读监督器实际执行） | 0 | 1 / 0 / 0 |
| `cargo test --locked -p astersql-pd-compat-tests --lib task_100_ -- --nocapture` | 0 | 3 / 0 / 0 |
| 28 个只读监督器入口和独立 admin marker 入口 | 均为 0 | 各 1 / 0 / 0 |
| `bash scripts/run-pd-compat-readonly.sh --case unknown-sa-022` | 2 | 参数拒绝 |
| `bash scripts/run-pd-compat-readonly.sh --mode rust --case gc-legacy` | 2 | 缺少双实现模式 |
| `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo metadata --format-version 1 --no-deps`、`cargo check --workspace --locked`、`git diff --check` | 均为 0 | 静态 |

本轮使用双实现模式与默认 Cargo features。Go PD 为已有 macOS amd64
`without_dashboard` 构建，Rust PD 为当前 macOS arm64 调试构建。
已有三个 Go 客户端工具为 macOS arm64 构建，逐个验证模块及固定 kvproto。
Go SDK、协议 ZIP、PROTOC、官方 etcd 3.5.15 和 etcdctl 均核对准入摘要。
GC 持久证据使用各实现真实 etcd WAL，原生后端仍由批次 17–18 交接。
完整工作区行为回归归批次 24。

每阶段动态取得共享槽位 1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
所有本项 Cargo 和服务子进程退出后释放自有锁，保留缓存。
1101 个 Go 源码及模块路径的原始字节和 Git 状态首尾一致。
Go 侧只运行已有可执行产物及只读版本检查，主计划、领域计划及来源史诗保持原状。

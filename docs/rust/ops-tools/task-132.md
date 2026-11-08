# 运维 132 默认发行的本地回执

在 `79643c4ca94467d73d0ecf4ba56e34bc5b73d35f`、Darwin arm64 和
Rust 1.99.0 上验证当前默认 `pd-server`。Rust 启动独立进程并连接官方
etcd 3.5.15。固定 Go PD 使用自身的 embedded etcd。此为 132 的局部行为证据，
正式目标待 131 交接后运行。完整 Dashboard/Swagger 装配仍由 125–131 和 133 验收。
[原始回执](task-132-evidence.json.gz)。

| 实际检查 | 结果 |
| --- | --- |
| `cargo build --release --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 退出 0，`release` 优化产物，约 1 分 38 秒 |
| `cargo tree --locked -p astersql-pd-cmd-pd-server -e normal,build` | 退出 0，1597 行，Dashboard、Swagger、Node.js、npm、pnpm、Yarn、Webpack 名称匹配数为 0。实际 [manifest](../../../cmd/pd-server/Cargo.toml) 未定义完整发行 feature |
| `cargo test --locked -p astersql-pd-server-config --lib dashboard_config_ -- --nocapture` | 退出 0，5 通过、0 失败、0 忽略、88 过滤。覆盖默认值、自定义及 TOML round-trip、既有 Go JSON 字节、标量和 null |
| [进程探针](check-minimal-release.py) | 退出 0，Rust 默认 release 与固定 Go PD 两个真实场景 |

| 实际观察 | Rust release | Go `without_dashboard` |
| --- | --- | --- |
| HTTP health | 200，`health: true` | 200，`health: true` |
| HTTP ready | `/pd/api/v2/ready` 200 | 以 health 和 version 等待服务可用 |
| HTTP / CLI 版本 | HTTP 200，`0.1.0`，CLI Classic | HTTP 200，`None`，CLI Classic |
| `/dashboard/` 与 `/dashboard/api/cluster/info` | 两处均 200，`text/plain; charset=utf-8`，响应字节 `Dashboard is not built.\n` | 两处均一致 |
| `/swagger/index.html` | 404，空响应 | 404，`404 page not found\n` |
| SIGTERM 和端口 | 退出 0，PD 与外部 etcd 的三个监听地址可重绑 | 退出 0，两个监听地址可重绑 |

健康响应的成员 ID、端口和 JSON 字段顺序保留在原始回执。版本值及 404 响应
按两侧实际值保留。就绪比较采用各自入口，Go 固定 oracle 的 v2 ready 未列入等价断言。
本地重试保留了三类夹具边界。Python 3.9 不支持 `hashlib.file_digest`，改为流式
摘要。Rust 过渡后端拒绝显式 `--data-dir`，目录参数只传给实际使用它的 Go PD 和
etcd。Go 启动初期在探测 v2 ready 时断开连接，最终探针先等待实际 health 和 version，
并仅对 Rust 要求 v2 ready。最终两个进程均成功退出。

## 复现

按原任务动态领取共享槽位，使用该槽位的绝对 `CARGO_TARGET_DIR`。
本次实际槽位 1，路径 `/Users/xiangmin/worker/code/pd/target/rust-slot-1`，
全部子进程退出后已释放。构建用已校验的 kvproto
`v0.0.0-20261016101550-f8ad079c8e98` ZIP 与 vendored protoc 3.2.0。
命令中的环境变量指向现有只读 Go 模块缓存、protoc、Go oracle 和 etcd。
本次 Go oracle 由 `go version -m` 确认使用 Go 1.26.8、amd64、
`without_dashboard`、提交 `b53bc9a015943526b5e61c6ae9e2ea1b87856972`，
`vcs.modified=true`。其字节摘要与 [冻结的 oracle](../server-api/acceptance/task-15.json.gz)
相同。Rust 二进制、Go oracle、etcd 摘要以及 Go build info 均在原始回执。

```sh
cargo build --release --locked -p astersql-pd-cmd-pd-server --bin pd-server
cargo tree --locked -p astersql-pd-cmd-pd-server -e normal,build
cargo test --locked -p astersql-pd-server-config --lib dashboard_config_ -- --nocapture
python3 docs/rust/ops-tools/check-minimal-release.py \
  "$CARGO_TARGET_DIR/release/pd-server" "$PD_COMPAT_GO_BIN" "$PD_COMPAT_ETCD_BIN"
```

Go 文件及模块首尾快照 1101 项一致，SHA-256
`22045dc3e33a47a167e35d8a4565bc3337dedb1e47a845e98967087b9e8899ed`。

## 完整发行缺口

本地锁定的 TiDB Dashboard 模块 `v0.0.0-20261013052755-e5fc397a69b0` ZIP 有
817 个条目，包含 UI 源码及 `empty_assets_handler.go`。归档无 `dist/` 和
`embedded_assets_handler.go`。模块 ZIP 的 SHA-256 为
`e66d5e8b6fbb1174039d3df3ac192ef27e3b1831b612dae4b5d1ec14df8102ce`。
当前工作区也缺生成的发行资产。[Dashboard 版本](../../../scripts/dashboard-version)
固定为 `9.0.0-e5fc397a`。[原有资源装配脚本](../../../scripts/embed-dashboard-ui.sh)
以对应的预构建归档生成 Go 文件，还会整理 Go 模块，故本轮仅检查已有归档和源码。
Rust 当前注册的是 [禁用回退](../../../pkg/server/http_server_config_write.rs)。
完整分支需要可运行 Dashboard backend、固定 UI 资源、实际 Rust 服务接线，以及
启用 Swagger/Dashboard 的只读 Go oracle。完整发行同时需观察真实审计及计量产物。

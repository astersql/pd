# 运维 91 的本地备份命令验证

以 `314bf98c2` 为基准，新增 Rust `pd-backup`。命令复用既有采集库的线性事务和
备份 JSON，保持 `-pd`、`-file`、`-cacert`、`-cert`、`-key` 的参数名。
`--etcd-endpoints` 用于 PD HTTP 和元数据端口分开的 Rust 部署。两个地址应属于
同一集群。省略该参数时使用 `-pd` 提供的端点。若提供多个 PD URL，首项用于
读取配置。TLS 证书配置用于两个 HTTPS 客户端。产物经同目录临时文件写入和同步
后原子替换，成功消息沿用 Go 命令，失败使用 stderr。
[命令入口](../../../tools/pd-backup/entry.rs)、
[传输](../../../tools/pd-backup/transport.rs)、
[采集](../../../tools/pd-backup/pdbackup/collect.rs)。

## 行为证据

| 命令 | 结果 |
| --- | --- |
| `cargo test --offline -p astersql-pd-backup-cli ops_091_entry_plaintext -- --nocapture` | 101，实际启动了待实现入口，退出 1，1 项失败 |
| `cargo test --locked -p astersql-pd-backup-cli ops_091_entry -- --nocapture` | 0，2 项通过、0 失败、0 忽略 |
| `cargo test --locked -p astersql-pd-backup-cli -- --nocapture` | 0，2 项通过、0 失败、0 忽略 |
| `cargo build --locked -p astersql-pd-backup-cli --bin pd-backup` | 0 |

测试使用真实 PD HTTP 和官方 etcd 3.5.15，对比归档 cluster ID、持久 ID 窗口、
TSO 非零及 HTTP 配置。明文用例实测权限拒绝并验证目录为空。双向 TLS 用例
实际认证 PD HTTP 和 etcd，移除客户端证书后退出 1，已发布文件逐字节保留。
动态路径只在成功消息中使用，备份断言按 JSON 字段比较。
[进程用例](../../../tools/pd-backup/entry_test.rs)。

## Go 工具对照

在 `tools/` 用现有 Go 1.26.8，设置
`GOENV=off GOWORK=off GOTOOLCHAIN=local CGO_ENABLED=0 GOPROXY=off GOSUMDB=off`，
`GOFLAGS=-mod=readonly`，并将 `GOCACHE`、`GOTMPDIR` 和二进制输出指向任务临时
目录。使用 `sandbox-exec` 禁止联网、禁止任务目录以外的写入，另外禁止任务目录内
写入 `.go` 和 `go.mod/go.sum/go.work/go.work.sum`。实际构建命令为
`go build -mod=readonly -o "$work/bin/pd-backup" ./pd-backup` 和
`go build -mod=readonly -o "$work/bin/pd-recover" ./pd-recover`，两者均退出 1。
现有模块缓存缺少 `etcd/client/v3@v3.5.15/client.go`、相应的 `.ziphash`，
固定 kvproto 的 Go 绑定也缺失。尝试对已解包目录按仓库 go.sum 校验以供离线
使用，零个目录通过。未生成 Go 工具产物，尚需同源工具二进制或完整的只读
SDK/依赖缓存。

Go 备份源码先创建目标文件，HTTP 配置请求使用默认客户端。另有待实测的时间戳
路径差异，Go CLI 的采集读取 cluster ID 后调用依赖包级 cluster ID 的
`keypath.TimestampPath`，当前调用链未找到初始化。Rust 直接使用读取到的
cluster ID。上述是源码边界，工具差分须用同源二进制运行并记录退出状态和
文件副作用。 [Go 入口](../../../tools/pd-backup/main.go)、
[Go 采集](../../../tools/pd-backup/pdbackup/backup.go)、
[Go cluster ID](../../../pkg/utils/keypath/cluster_id.go)。

Cargo 各阶段均动态领取槽位 1，绝对 `CARGO_TARGET_DIR` 为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`，结束后释放。Go 源码及模块
只读首尾快照共 1101 项，SHA-256
`22045dc3e33a47a167e35d8a4565bc3337dedb1e47a845e98967087b9e8899ed`。

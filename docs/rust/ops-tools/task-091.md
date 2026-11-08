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

## 续跑取得同源 Go 工具

公开的 asterSQL 仓库没有可用发布资产或 Actions 工件，本机也没有可运行的
已有 OCI 镜像。使用 Go 1.26.8 的 Darwin arm64 SDK 归档和 236 个模块
ZIP。SDK SHA-256 为
`a012b25b571bd0138a03dcd25375ceba866fe5ca822f426d2c66a4de56fd3f4b`。
模块 ZIP 内容和模块清单逐一匹配现有 `go.sum`，固定 kvproto ZIP 也匹配
原 SHA-256。将归档条目直接装入只读 UDF 镜像，构建只读访问镜像。此路径
解决了 WebDAV 在 Go 包扫描时的两处 I/O 错误。

在 `tools/` 使用 `GOPROXY=off GOSUMDB=off GOTOOLCHAIN=local GOWORK=off
CGO_ENABLED=0`，把 `GOROOT` 的源码和汇编头、`GOMODCACHE` 指向只读
镜像，输出和编译缓存指向任务临时目录。沿用原 sandbox 的网络和写入限制。
`go build -p 12 -mod=readonly -tags=without_dashboard -o "$work/bin/pd-backup" ./pd-backup`
和对应 `pd-recover` 命令均退出 0。沙箱拦截了 Go 的可选 VCS stat 缓存写入，
构建继续成功。镜像已卸载。

| 工具 | SHA-256 |
| --- | --- |
| Go `pd-backup` | `1c42a8870f268e688121cde634ebe8a36cfbaf718624461992c7b1b26284f5f4` |
| Go `pd-recover` | `7a8a5cd62f9c5decc05aad0fdcf796e4becece9c7afe1fe344a579d204f463a3` |

`go version -m` 确认两个真实入口、Go 1.26.8、darwin/arm64、固定
kvproto `f8ad079c8e98` 和构建提交 `7b60522a95454ca8e90a9cde2bba0ad3ab87a517`。
以固定 Go PD 的 `b53bc9a015943526b5e61c6ae9e2ea1b87856972` 对比工作区，
受保护的全部 Go 源码和模块路径无差异。

在同一真实 Go PD 的 embedded etcd 上执行两个备份进程。源 cluster ID 为
`7705791638910230066`，实际时间戳在 `/pd/7705791638910230066/timestamp`。
Go 命令退出 2，在 `backup.go:84` 报索引越界并留下 0 字节文件。Rust 命令
退出 0，读取实际时间戳键并发布 JSON。

在同一隔离夹具显式写入 `/pd/0/timestamp` 作为诊断输入，值复制自上述实际
时间戳键。Go 命令随后退出 0。两个输出的 cluster ID、ID 窗口 `1000`、
TSO 窗口 `1794144432788440000` 完全一致，配置的顶层键集合也一致。
诊断输入用于定位 Go 命令依赖的包级 cluster ID。实际 Go 拒绝和文件副作用
已记录，Rust 保留按实际 cluster ID 取值及原子发布的行为。
[进程输出和键值](task-091-092.json.gz)。

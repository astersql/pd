# Classic PD 本地原生拓扑

同一个 Rust `pd-server` 默认使用外部 etcd。传入两个本地参数后，独立入口
调用既有 `open_isolated` 打开 SQLite/OpenRaft，并将同一个 `MetaStore` 交给
classic 共享进程装配。独立入口负责信号和存储回收。嵌入宿主使用同一个
`PdNodeBuilder`，由调用方持有 Tokio 执行器、监听器、Registry 和存储生命周期。
[独立入口](../../../../cmd/pd-server/main.rs)、
[本地后端装配](../../../../cmd/pd-server/local_native.rs)、
[共享进程装配](../../../../cmd/pd-server/classic_process.rs)、
[嵌入宿主](../../../../pkg/runtime/node.rs)。

```sh
"$CARGO_TARGET_DIR/release/pd-server" \
  --name=pd-local \
  --client-urls=http://127.0.0.1:2379 \
  --peer-urls=http://127.0.0.1:2380 \
  --local-native-data-dir="$PD_LOCAL_STORE" \
  --local-native-cluster-id=18
```

`PD_LOCAL_STORE` 指向本地专用目录。首次启动使用不存在或为空的目录，重开
沿用同一目录、共识 ID、成员名及监听/公告地址。共识 ID 是非零整数，与 PD
自己的持久 cluster ID 分开。既有存储必须匹配共识 ID 和节点 1。目录拒绝其他
内容，存储自身持有独占锁；同一目录已有进程时第二个进程退出。
`--config-check` 检查配置，磁盘校验在实际启动时执行。

本地模式限 classic、单 voter、无 Raft 网络。客户端和 peer 的绑定/公告配置
均须使用带端口的 loopback IP。peer 地址用于成员信息；共识不监听该地址。
外部 etcd、etcd 成员诊断、join、embedded-etcd 配置以及 API service mode

本地验收覆盖实际 RPC/HTTP、宿主隔离、SIGKILL 后 WAL 重开及 SIGTERM
关闭。生产认证、成员管理、切换回滚与发布仍按现有计划验收，主题 9
继续作为生产实施和发布的前置。
[本地回执](task-18.md)、
[生产 ADR](../../production-meta/adr.md)、
[生产准入](../../production-meta/task-01.md)、
[主题 9 交接](../../ops-tools/acceptance.md)。

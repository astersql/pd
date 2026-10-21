# 最小 Rust PD 运行主链

当前实现是单节点 classic PD 的纵向 MVP，使用真实 `EtcdMetaStore`。
外部 etcd 是过渡依赖；这不代表完成 embedded etcd 的生产替代。
不支持 follower 转发、多成员拓扑、TLS、region heartbeat、调度或 MCS。
未移植的 PD RPC 由 tonic 默认实现明确返回 `UNIMPLEMENTED`。

## 启动与嵌入

先按任务的 Cargo 共享槽位规则领取槽位并设置绝对 `CARGO_TARGET_DIR`：

```sh
cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server
"$CARGO_TARGET_DIR/debug/pd-server" \
  --etcd-endpoints=http://127.0.0.1:2379 \
  --client-urls=http://127.0.0.1:2381 --name=pd
```

进程入口拥有 Tokio runtime 和 SIGINT/SIGTERM。嵌入宿主调用
`bootstrap_config` 装载持久化集群 ID，然后向 `PdNodeBuilder::new(handle, cancel)`
的 `start(store, config, listener, registry)` 传入共享资源。库不安装全局信号、
日志订阅器或私有 executor。名字生成稳定 member ID，bootstrap 会校验持久化身份；
更改地址或身份需要显式迁移，不能静默覆盖。

启动顺序：storage → leadership → services → listener → ready。
关闭顺序：stop-admission → background-joined → leadership-released → storage-released。
`shutdown()` 可重复调用，等待关闭；`Drop` 请求取消，宿主需要等待
`shutdown()` 或 `wait()` 才能确定资源已回收。宿主取消令牌不会被节点反向取消。
领导租约每秒续期，TSO 每 50ms 更新；续期/时间窗口失败使服务停止并由 `wait()`
返回错误。初始化失败会释放本次租约与监听器。

HTTP `/pd/api/v1/health`、`/pd/api/v1/members` 和 PD gRPC 共用同一个 listener。
健康和成员请求执行真实 leadership fencing；ID 和 TSO 使用持久化窗口。
当前 ID RPC 只支持单个 ID，批量请求返回 `UNIMPLEMENTED`。
TSO 支持全局流、校验 cluster ID/count，取消或客户端断开即销毁响应流，
没有为每条流分离后台任务。

共享 Registry 注册 `pd_rust_requests_total` Counter（无 labels），统计进入 ID/TSO
领域层的请求尝试，关闭时注销。新名称不替代 Go 指标；共享 registry 的重复注册
返回启动错误，不在可嵌入库里 panic。这里只暴露 registry，不承诺完整 Go metrics API。

兼容测试使用的 Go CLI 元数据参数 `--data-dir`、`--initial-cluster`、advertise URLs
不用于创建 Go/本地存储；当前服务只开放 `--client-urls` 指定的一个 HTTP 明文监听器。

## 行为基准与验收边界

Go oracle 是 `server/api/{health,member}.go`、`server/grpc_service.go` 的
GetMembers、AllocID 和 Tso；协议使用固定 kvproto commit。测试调用真实 Go/Rust
进程、curl、tonic 客户端及独立 etcd 测试进程，不用固定响应服务替代 Rust 验收。

```sh
cargo test --locked -p astersql-pd-server -p astersql-pd-runtime \
  -p astersql-pd-cmd-pd-server -p astersql-pd-compat-tests -- --nocapture
./scripts/run-pd-compat.sh --mode dual --case health-member --case tso
```

health-member MVP 差分比较 HTTP 状态、完整 health 数组、响应 header、成员数量、
成员及 PD leader 的 name/member_id/client_urls/peer_urls、leader_priority、dc_location，
以及 tso_allocator_leaders，并验证 ID 和地址关联；未审查的新字段会使差分失败。
cluster/member ID 与监听端口按观察值归一化。此基准显式不比较 Go embedded-etcd
`etcd_leader` 拓扑、成员 binary_version/git_hash/deploy_path，
也不比较范围外 config API；因此不能据此宣称完整 GetMembers API 等价。
原来的完整响应/config 差分 helper 和反例检测测试仍保留。

TSO 差分在同一个双向流中请求 1、3、1，逐项验证严格单调、同 physical 时的
logical 增量和跨 physical 时的 logical 重置。比较归一化 physical 原点及 logical
增量后的 protobuf 内容、header、count、suffix_bits；0 count 的 gRPC code/message
逐字比较。物理时钟与 tick 调度不同，不直接要求独立进程的绝对时间或 logical
累计值相等。客户端先发送第一条请求，再等待响应 header，兼容 Go 的延迟发头行为。

真实 etcd 测试还覆盖重复 shutdown、端口/租约立即复用、启动失败回收、租约丢失
停止服务、保持打开的流关闭，以及共享 registry 的注册/注销。独立进程差分用
SIGTERM 要求成功退出，随后检查端口关闭和临时目录删除。

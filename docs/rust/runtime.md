# Rust PD 运行链和验收边界

独立进程和嵌入宿主共用 `PdNodeBuilder`、领域句柄、MetaStore和宿主Registry。
生产入口使用真实 `EtcdMetaStore` 与外部etcd；测试fixture直接启动官方etcd二进制，
不会因此成为原生Rust后端或Go embedded-etcd拓扑的等价实现。

当前服务已超出最初的单节点MVP：已有集群与心跳装配、调度worker、成员管理、
follower转发、TLS、HTTP配置与管理API、指标和关闭流程。
[任务100](server-api/task-100.md)记录41个classic PD方法的历史回归范围，
[任务101](server-api/task-101.md)记录HTTP/配置/指标目录及其明确的后续依赖。
这些局部证据不能推导为全部领域兼容；[任务102](server-api/task-102.md)记录当前
修复、实际重跑结果、oracle版本限制及尚未解除的最终准入缺口。

## 启动与嵌入

先按任务的共享槽位规则领取槽位并设置绝对路径 `CARGO_TARGET_DIR`：

```sh
cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server
"$CARGO_TARGET_DIR/debug/pd-server" \
  --etcd-endpoints=http://127.0.0.1:2379 \
  --client-urls=http://127.0.0.1:2381 --name=pd
```

进程入口拥有Tokio runtime、信号和日志初始化。嵌入宿主通过 `bootstrap_config`
装载持久化集群ID，再调用 `PdNodeBuilder::new(handle, cancel).start(...)`，传入
共享存储、配置、listener和Registry。库不安装全局信号或日志订阅器，也不创建私有
executor。持久化身份包含成员和地址，重启或重建宿主需要保留身份，变更需显式迁移。

服务在同一listener上提供HTTP与gRPC；TLS、认证、转发和准入逻辑由宿主装配。
`--data-dir`、`--initial-cluster` 等兼容CLI参数不意味着Rust入口启动了Go embedded
etcd或本地生产后端。实际元数据仍来自 `--etcd-endpoints`。

宿主创建并管理worker和Registry scope。`shutdown()`/`wait()`负责等待关闭，
`Drop`仅请求取消；重复关闭应安全。领导租约、TSO、集群观察与调度worker服从宿主
和term取消。启动失败需要回收已取得的资源，正常关闭后释放listener、租约和指标。

## 已接通的maintenance边界

`/pd/api/v2/maintenance` 的四种注册方法复用现有StorageEndpoint，与其他服务共享
同一个MetaStore。全局锁在事务中排斥不同任务类型；查询、冲突、删除和坏记录处理
有独立真实TCP测试。etcd停止、WAL重启和客户端重建后仍可读取任务，
`pd_maintenance_task_info`仅在成功变更后更新，宿主退出注销。
本轮真实Go/Rust双进程差分覆盖11个成功/错误场景，原始响应和时间归一化口径见任务102。

GC、keyspace、TSO/meta-service组和MCS的必要领域能力仍有缺口；明确503拒绝或
生成的UNIMPLEMENTED不算兼容实现。主题8/9/10依赖、原生后端durability与embedded
拓扑不能因外部etcd测试成功而被声明完成。

## 只读验证入口

验证需要先领取Cargo槽位；所有子进程继承该槽位。可设置
`PD_COMPAT_ETCD_BIN=/absolute/path/to/etcd` 复用已存在的官方3.5.15二进制，
Rust fixture与CLI只读supervisor均可直接启动它，不必现场编译Go fixture。
HTTP/member supervisor另使用 `PD_ETCD_BIN`，并要求同目录有etcdctl。
未设置native入口时，保留原有已安装Go工具链与只读源码构建fixture的路线。

双进程比较须提供绝对路径 `PD_COMPAT_GO_BIN`、
`PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server"` 和 `PD_COMPAT_MODE=dual`。
完整协议验收的Go oracle必须匹配固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`；
一个较旧发布二进制可验证未变化的maintenance契约，但不能证明较新protobuf字段兼容。

```sh
cargo test --locked -p astersql-pd-server --lib task_102_ -- --nocapture
./scripts/run-pd-compat-readonly.sh --mode dual --case health-member --case tso
```

Go源码和模块文件保持只读；不要使用会注入failpoint、生成Go源码或整理Go模块的
旧验证入口代替上述流程。显式 `GOMODCACHE` 可以提供已有的固定协议ZIP，Rust
构建仍核对原校验和且只提取proto。测试通过数、跳过项、失败和清理结果分别记录，
不把ignored、零测试或缺少oracle的运行计作完整兼容通过。

## 最终回归范围（2026-11-04）

固定协议的 amd64 `without_dashboard` Go oracle 与现有 Rust 服务完成本轮真实
双进程回归；arm64 的超范围数值转换不并入 amd64 合同。串行测试下，完整 workspace
2252通过、82默认忽略，四crate联合468通过、82默认忽略，均退出0；52个HTTP/配置/
指标supervisor全部实际成功。必要ignored差分由原supervisor显式执行并单独计数，
最后52份成功报告的指标属性重验2通过，观察到53个shared family schema。

验证使用官方etcd 3.5.15及相邻etcdctl：health/member在停止前后检查真实leader键，
并检查进程成功退出、目录与端口释放。TLS拒绝请求的持久副作用比较只排除对应集群
后台TSO时间戳行，其余键和值/revision保持精确比较。详细失败、修复、命令与输入
指纹见[任务102](server-api/task-102.md)。完整回归不解除GC/keyspace/MCS和原生
生产后端durability/embedded拓扑的独立准入缺口。

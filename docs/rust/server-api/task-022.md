# 任务022：Store 注册与查询

状态：已完成。基准 `e58f1b8a4db58bd3eb5e5f07b6d76db0c7bb0872`；依赖任务021及任务1–4清单、只读监督入口已核验。默认 features、classic PD、majority 配置和外部 MetaStore 过渡架构；不将其描述为 Go embedded-etcd 的等价实现。

## 来源与实现

[source-inventory.md](source-inventory.md)任务22行、[wire-inventory.md](wire-inventory.md) GetStore/PutStore/GetAllStores 三行由本证据覆盖；[surface-inventory.md](surface-inventory.md)没有 owner=22 的独立配置、指标、CLI 行。固定协议 commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

只读 Go 映射：`server/grpc_service.go:774–924`、`server/cluster/cluster.go:1556–1714,2008–2020`、`pkg/core/store.go`、`pkg/utils/grpcutil/cluster.go:269–299`。GetStore 已读取同一个 canonical cache；PutStore/GetAllStores 原先落到 tonic 默认 Unimplemented。

- [grpc_store.rs](../../../pkg/server/grpc_store.rs)实现注册与枚举协议入口，复用既有角色/cluster ID/bootstrap 校验；取消优先于 pending 操作。GetStore 在 [grpc.rs](../../../pkg/server/grpc.rs)保留同一缓存查询并增加 pending 读取消。[SyncGrpc](../../../pkg/server/sync_transport.rs)显式转发两个新方法，未建立第二个服务。
- [PdServices](../../../pkg/server/lib.rs)增加领域元数据接口；[RoleServices](../../../pkg/runtime/member_role_worker.rs)逐请求取得当前 term；[Application](../../../pkg/runtime/node.rs)和角色门均转到 [ClusterHost](../../../pkg/runtime/cluster_host.rs) 的同一资源与状态。复用 `StoreInfo`、`merge_labels`、engine 校验和 `StorageEndpoint::save_store_meta`，proto 转换留在 server。
- 注册在宿主资源锁内完成校验、持久化和发布；同一锁也覆盖现有 Store 状态 worker。写失败不发布；更新时合并标签，保留生命周期、物理销毁状态、心跳与统计；不以 incoming metadata 覆盖整个现有 StoreInfo。已 Removed 的ID拒绝重新注册。地址与非空 peer_address 双向交叉检查，允许复用 Removed/physically_destroyed store 的地址。
- 列举按 NodeState::Removed 过滤，不错误地用旧 StoreState 替代；GetStore 仍可读 Removed 元数据。错误 header 保留 cluster ID，StoreTombstone/InvalidValue/Unknown 分别映射；地址冲突完整文本使用固定 Store schema 的 Go compact text 格式，覆盖 label、枚举、可选 range、UTF-8 八进制转义。

## 行为证据

[grpc_store_test.rs](../../../pkg/server/grpc_store_test.rs)显式注册 `task_022_behavior`，通过真实 etcd + PdNode + TCP/tonic 运行[共享场景](../../../pkg/server/grpc_store_scenarios_test.rs)：

- 未 bootstrap 的 Put/GetAll 返回 NOT_BOOTSTRAPPED；成功注册、GetStore 全部字段、majority replication status；空消息/缺失消息、错误 cluster ID、缺失ID和非法 engine 错误。
- 四种 address/peer_address 冲突均返回 UNKNOWN 且不新增失败 Store；更新标签按不区分大小写的 key 合并，保留未更新标签及事务版本范围；禁止用请求状态字段改写既存生命周期。
- Removed 仍可查询，不能重注册，枚举可排除；Removed 和物理销毁地址可复用。12个并发新ID争抢同一地址，恰好一个成功。
- RPC枚举与真实 etcd 保存的 Store protobuf 全字段相等；同监听器 HTTP ping 成功。心跳容量通过 GetStore stats 可见；心跳不被错误地要求立即持久化。保持身份和 MetaStore 重启后，7条元数据（含并发赢家）恢复相等。

[grpc_store_fault_test.rs](../../../pkg/runtime/grpc_store_fault_test.rs)注册 `task_022_transaction_failure_and_cancel`：宿主 fixture 使用同一个真实 ClusterHost，单独 TCP adapter 的取消令牌用于隔离传输取消；没有伪造成功存储值。真实存储事务边界追加2MiB写入，让 etcd 实际拒绝超限事务，返回 UNKNOWN、旧缓存/持久地址不变、故障 key 不落盘。第二个事务在外部IO前暂停，取消 adapter 后返回 Unavailable，Store5的持久键和缓存均不存在；新入站请求被拒绝，原宿主可以重新注册Store5，证明中断的 handler 释放资源锁。此故障/取消场景不宣称已经取得 Go 同场景差分。

初始聚焦命令编译并运行1项测试，退出101：`PutStore must be served` 断言收到 Unimplemented；不是编译失败。开发期间修正了协议类型命名和标签方法名、server dev-dependency 重复 crate identity 的测试装配、心跳非立即持久化的断言，以及停止整个 etcd 导致退位的故障设计。最终使用真实 etcd 超限事务，隔离元数据事务失败。runtime 包级回归还发现旧 `shared_runtime_network_lifecycle` 预期 GetAllStores 的默认桩，已更新为实际 cluster ID 校验与 NOT_BOOTSTRAPPED 契约；没有放宽成功断言。

[grpc_store_differential_test.rs](../../../pkg/runtime/grpc_store_differential_test.rs)通过任务4只读监督入口 `--case store-registration` 启动已有 Go binary 和真实 Rust node。双方运行同一个共享 RPC 场景，比较全部 Store 元数据、4种完整冲突文本（含Unicode和可选range）、engine错误文本及状态、tombstone错误、空消息错误、错误 cluster 码，并验证过滤与失败请求无缓存副作用。双方重启各自已有持久层，6条 Store 完全相同。仅对无序枚举排序；cluster ID 在各自断言，不抹去其他字段。Rust专有的12并发、存储故障及传输取消不冒充 Go 差分。

Go oracle 是既有 `bin/pd-server`，build info 为 go1.26.8、无 VCS revision，不宣称来自当前HEAD。沿用只读 etcd fixture 构建入口，Go build 使用 `-mod=readonly`、`GOWORK=off`、`GOTOOLCHAIN=local`，不运行 make/failpoint/Go格式化/模块整理；输出仅为槽位内二进制。Go 与模块文件均由前后 SHA-256/存在性/Git 状态守卫验证。

## 验证

所有阶段依次通过原子 mkdir 动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；每阶段记录任务、会话和持有PID，结束释放自己的锁，保留缓存。修改Rust后先运行 `cargo fmt --all`，未增加 feature、依赖或平行 target-dir。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_022_ -- --nocapture` | 101 | 1行为失败 |
| 最终同一聚焦命令 | 0 | 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_022_ -- --nocapture` | 0 | 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 126单元+1集成通过 |
| 首次 `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 101 | 28通过、旧桩预期1失败；16忽略不计通过 |
| 更新旧测试后同一runtime命令 | 0 | 29通过，16忽略不计通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 格式 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case store-registration` | 0 | 1通过，0忽略 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 1983通过，17忽略不计通过 |

最终监督报告 `compat-readonly-1793260690146798000.json`：退出0，readonly_unchanged、inputs_unchanged、work_directory_cleaned、process_group_cleaned 均为 true。Go保护集合1983文件，逐文件SHA-256、存在性及Git状态不变，包含ignored/untracked目录；plan.md未修改。Python AST、文档相对链接及git diff --check通过。仅有既存cpu_usage废弃警告。

[压缩证据](task-022-evidence.json.gz)保存失败/通过原始日志、确切命令/退出码/有效测试数、Go守卫与只读监督报告、最终代码哈希。验证进程结束后释放自己的槽位锁。提交只包含任务22变更；保留其他编号任务的既存删除，不修改总计划或来源史诗。

压缩证据 SHA-256：`6abe3bd0e92809596a71a8253e5b91209f55160df4621751f996796152ddfe6f`。

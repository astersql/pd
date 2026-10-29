# 任务020：集群 Bootstrap 事务

状态：已完成（默认 classic PD / majority；外部 MetaStore 过渡架构）。基准 `5634d9039`，依赖任务019及任务1–4清单和只读监督入口已核验。没有修改 Go、模块文件或只读 plan.md。

## 实现与来源

[source-inventory.md](source-inventory.md) 第20任务、`server/grpc_service.go` Bootstrap/IsBootstrapped、`server/cluster/cluster.go` bootstrap time、`pkg/utils/keypath/absolute_key_path.go` cluster/store/region/time 路径行；[wire-inventory.md](wire-inventory.md) 两个 owner=20 RPC 行均由本证据覆盖。[surface-inventory.md](surface-inventory.md)没有独立 owner=20 的配置、指标或 CLI 行。

额外直接核对 Go `server/util.go:checkBootstrapRequest` 和 `server/server.go:bootstrapCluster`：先角色和 cluster ID 校验；已引导检查先于请求内容校验；缺失元数据、零 ID、非全域键范围、非单 peer、peer store 不匹配均返回成功 RPC 中的 UNKNOWN 错误 header。重复引导返回 ALREADY_BOOTSTRAPPED；并发抢占失败不写入数据。

- [grpc_bootstrap.rs](../../../pkg/server/grpc_bootstrap.rs)实现协议校验、错误 header 和两个 RPC，沿用 request_role admission；[SyncGrpc](../../../pkg/server/sync_transport.rs)转发至同一处理器。
- [StorageEndpoint](../../../pkg/storage/endpoint/cluster_meta.rs)复用既有 store/region 持久化编码，在一次条件事务中创建 cluster、纳秒 bootstrap time、store、region。MetaStore 保持后端中立；没有把 proto 引入领域接口。
- [RoleServices](../../../pkg/runtime/member_role_worker.rs)使用现有 TermStore，将 leader fencing 与 bootstrap 条件合并到真实事务；[ClusterHost](../../../pkg/runtime/cluster_host.rs)复用宿主资源锁，事务成功后直接发布 store/region 至同一 canonical cache。失败不发布。重启沿用既有恢复流程。
- max_peer_count 取宿主持久配置。成功响应包含 MAJORITY 状态；当前没有 DR 自动同步 manager，宿主配置为其他复制模式时显式拒绝引导，不伪造 DR 状态。本任务没有扩展主题8复制管理。Go embedded-etcd 与 Rust 外部 MetaStore 的拓扑边界仍遵守 ADR-001。

## 行为与差分

[grpc_bootstrap_test.rs](../../../pkg/server/grpc_bootstrap_test.rs) 显式注册两个 `task_020_` 测试，使用真实 etcd、TCP/tonic 和 PdNode：

1. 引导前 false；非法请求无成功；两个并发调用恰好一个成功；重复返回 ALREADY_BOOTSTRAPPED；四项持久数据具有相同 create_revision；初始 store 即时可读；关闭再绑定同一持久身份重启，状态及 region 可读。
2. TCP 转发器先完成真实节点初始化，再冻结已连接流量；100ms 在途 Bootstrap 被客户端超时丢弃。回收转发任务和节点后，停止真实 etcd，在真实 bootstrap 事务边界取得存储错误；重启 etcd 后无任何部分初始键。重试事务成功，第二次失败且 entries 原样不变。已取消 handler 在 host admission 前返回 Unavailable，测试用 panic 守卫证明未访问宿主。所有 relay 连接任务 abort 并 join。

初始失败命令实际运行1项，退出101：`IsBootstrapped must be served`，得到 Unimplemented。不是编译失败或零测试。扩展测试曾暴露测试重启改了持久 client_url（IdentityConflict），已改为同地址重启；取消测试曾因关闭后端立即断连而不能证明在途超时，已改为冻结连接。最终证据使用修正后的测试。

[grpc_bootstrap_differential_test.rs](../../../pkg/runtime/grpc_bootstrap_differential_test.rs) 通过任务4监督入口 `--case bootstrap` 启动已有 Go 二进制和真实 Rust PdNode。双方比较8类非法请求的 UNKNOWN 类型与消息、错误后仍未引导、成功 header、MAJORITY 全部状态字段、重复错误、IsBootstrapped、完整初始 store/region 读回，以及错误 cluster header 的 transport code。只归一化随机 cluster ID；输入 store/region 相同。存储故障与取消的额外真实 Rust 证据单列，不冒充 Go 同场景故障差分。Go 二进制 build info 无 VCS revision，不声称来自当前 HEAD。

## 验证

所有 Cargo 阶段依次原子 mkdir 动态领取到槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，默认 features。锁中记录计划、任务、会话和持有 PID；阶段结束释放自己的锁，保留缓存。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_020_ -- --nocapture` | 101 | 1行为失败 |
| 最终同一聚焦命令 | 0 | 2通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 123单元+1集成通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 格式化 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 1977普通+2doc-tests，共1979通过；15忽略不计成功 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case bootstrap` | 0 | 显式1通过、0忽略 |

workspace 回归覆盖生产事务及宿主接线；随后仅对齐并发冲突的 Go 错误文本，并加强 HTTP ping、cluster ID/max_peer_count 和纳秒引导时间断言。最终再次执行格式化、聚焦、完整 server、server check、二进制构建及只读差分，全部通过。已有 cpu_usage 废弃字段警告没有新增或禁用检查。

首轮差分行为通过，但运行期间测试文件继续编辑，监督报告的 inputs_unchanged=false，因此不作为最终接受证据。最终报告 `compat-readonly-1793243508883092000.json` 退出0；readonly_unchanged、inputs_unchanged、work_directory_cleaned、command.process_group_cleaned 全为 true。

[压缩证据](task-020-evidence.json.gz)保存完整命令日志、Go 守卫、监督报告及最终源码哈希。开工源码保护集合1101文件与结束时一致；完整集合（包括 ignored/untracked 和构建缓存内的 Go 文件）1983文件，与任务019已保存的完整基线及本次两轮监督快照完全相同。Go/模块 Git 状态为空；只读 plan.md 与 HEAD 字节相同，Cargo.lock 没有改动。Python AST、Bash语法、证据文档链接和 git diff --check 均通过。

验证进程和 relay worker 已结束，自己的槽位锁已释放，缓存保留。未暂存其他编号任务的既有删除；任务020在报告完成证据后删除，不修改 plan.md。

证据压缩包 SHA-256：`b4e72c26292c1b2f8ba85e43b86cdca90987cba1d78495af676fe3c9ea8b6249`。

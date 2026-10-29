# 任务023：Region 点查询

状态：已完成。基准 `bca7be1e7`；批次22及任务1–4已核验。固定 kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认 features。

## 来源及实现

本证据覆盖 [source-inventory.md](source-inventory.md) 的任务23行及 [wire-inventory.md](wire-inventory.md) 的 classic PD GetRegion/GetPrevRegion/GetRegionByID 三行。[surface-inventory.md](surface-inventory.md) 无单独 owner=23 行。Go 只读映射：`server/grpc_service.go:1513–1602,2181–2203,2775–2790`、`pkg/utils/grpcutil/cluster.go:35–118,302–308`，及既有 `pkg/core` 区间索引行为。主题6准入引用 task-001 的历史核验，不把历史证据当成本次运行差分。

- [grpc_region_lookup.rs](../../../pkg/server/grpc_region_lookup.rs)统一三种入口；[grpc.rs](../../../pkg/server/grpc.rs)与[SyncGrpc](../../../pkg/server/sync_transport.rs)显式转发。服务取消优先于 admission/cache await；角色校验先于 cluster ID，未 bootstrap 返回协议 header。
- [RegionsInfo::region_by_key](../../../pkg/core/regions_shared.rs)在单个索引锁内返回原始 Arc，复用已有 search/search_prev，不复制区间算法；[ClusterHost](../../../pkg/runtime/cluster_host.rs)和[RoleServices](../../../pkg/runtime/member_role_worker.rs)逐请求使用当前 canonical leader cache。所有 proto 转换保留在 server。
- follower 要求请求元数据键 `pd-allow-follower-handle` 存在；值为 false 或空字符串也表示允许。复用既有 SyncFollower/ClientApply，通过真实 SyncRegions 连接当前 leader；leader watch、重试和同步在宿主同一取消作用域，watch 变化/退出会 drop 同步 future，未新增 detached worker 或私有 runtime。同步完成前及缺失返回 REGION_NOT_FOUND；leader 缺失返回无 error 的空结果。follower 同步投影保留 Go 的 region/leader 子集，不凭空复制 down/pending peers。
- follower 的非权威投影与 checkpoint 使用内存 MetaStore；绝不向共享的 leader-fenced 生产命名空间写入。重启后重新同步，不宣称 follower 本地持久化、Go embedded-etcd 拓扑或新生产本地后端已经等价。节点发现/权威 leader 元数据仍使用同一个真实外部 etcd。
- Go `pkg/schedule/config/store_config.go:144–149` 的默认 store 配置关闭 region buckets，need_buckets 不再触发 Unimplemented。宿主配置 gate 启用时按请求返回完整有效 buckets；follower 始终不返回 buckets。启用场景在真实 ClusterHost 测试中验收，不冒充 Go 默认配置差分。

## 行为与差分

[独立行为测试](../../../pkg/server/grpc_region_lookup_test.rs)显式注册 `task_023_behavior`，真实 etcd、双节点、同监听器 HTTP ping 和 TCP/tonic 运行[共享场景](../../../pkg/server/grpc_region_lookup_scenarios_test.rs)：bootstrap、三条 heartbeat region、leader/down/pending peers、半开边界、空 key、二进制 key、范围空洞、相邻/不相邻 predecessor、失效与缺失 ID、need_buckets 两个值、错误 cluster ID、follower 准入及冷缓存。每角色44条完整响应，并检查查询前后持久 region entries 不变。

[故障测试](../../../pkg/runtime/grpc_region_lookup_fault_test.rs)复用任务22的真实 backend IO 门与 Host helper。先向真实 canonical heartbeat cache 写入带 bucket metadata 的 region，启用配置后比较 key/ID 返回及 buckets 开关。随后在真实 registration 事务的外部 IO 前暂停，资源锁覆盖三种点查询；取消 adapter 后所有 pending 读取与写入返回 Unavailable，失败 Store 的持久键与缓存都不存在。没有用 mock 调用次数证明成功。

[差分测试](../../../pkg/runtime/grpc_region_lookup_differential_test.rs)由任务4只读监督入口 `--case region-lookup` 显式运行，启动既有 Go binary 的 leader+join follower 与 Rust 双节点。双方各角色44条完整 protobuf 字节相等，仅将各自已断言正确的 cluster ID 归零；成功、错误 header、边界与 follower 行为均同场景验证。Go oracle 为已有 `bin/pd-server`，build info 是 go1.26.8，没有 VCS revision，不声称来自当前 HEAD。启用 bucket gate 与真实事务阻塞取消为 Rust 专项证据，未宣称 Go 同场景故障差分。

初始聚焦测试可编译运行，1项行为断言收到 GetRegion Unimplemented，退出101。开发期修复了 bucket stats QPS 字段、局部接口重命名和测试空地址冲突；首次只读差分因测试流建立等待被取消并清理，随后相同共享场景通过。首次 server 包级回归为126通过/1失败：旧 task020 故障测试在切断代理后无条件 unwrap shutdown，收到真实 Store(Unavailable)。现在仅接受 shutdown 成功或确切 MemberError::Store(StoreError::Unavailable)，其他错误仍失败，未放宽事务/取消断言。

## 验证

动态原子 mkdir 从1–10领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。每个 Cargo 命令与只读监督子进程继承该绝对路径，未使用 --target-dir。Rust 修改后先运行 cargo fmt --all。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_023_ -- --nocapture` | 101 | 1行为失败；非编译错误 |
| `cargo test --locked -p astersql-pd-runtime --lib task_023_ -- --nocapture` | 0 | 1通过，0忽略（含启用bucket及3种取消） |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态/构建，不计行为测试 |
| `cargo fmt --all -- --check` | 0 | 静态/构建，不计行为测试 |
| `cargo test --locked -p astersql-pd-server --lib task_023_ -- --nocapture` | 0 | 1通过，0忽略（不计通过） |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 128通过，0忽略（不计通过） |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 30通过，17忽略（不计通过） |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态/构建，不计行为测试 |
| `cargo check --workspace --locked` | 0 | 静态/构建，不计行为测试 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 1985通过，18忽略（不计通过） |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 静态/构建，不计行为测试 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case region-lookup` | 0 | 1通过，0忽略（不计通过） |

Rust 修改后 `cargo fmt --all` 退出0。Python AST、入口 bash 语法、相对文件链接、`git diff --check` 均通过。未修改依赖或 Cargo.lock；仅有既存 cpu_usage deprecated 警告。

最终压缩证据收录完整初始失败/通过日志、包级首次失败、最终 workspace 原始日志、每次只读监督报告、开始/结束 Go 文件存在性及 SHA-256/Git 状态、最终源文件哈希（排除本证据文档与其自引用压缩包）。只读 supervisor 报告的 readonly_unchanged、inputs_unchanged、work_directory_cleaned 和命令 process_group_cleaned 均为 true。

Go 保护集合1983文件，含 ignored/untracked，前后完全相同；plan.md 与基准 HEAD 完全相同。所有验证进程结束后释放本任务槽位 owner/锁目录，保留编译缓存。提交只包含本任务变更；其他编号任务既存删除保持不动。

最终监督报告：`compat-readonly-1793262534197041000.json`。

[压缩证据](task-023-evidence.json.gz) SHA-256：`fe2fde7804060a7984c6232491ee8088e0502c0dc530a0717e0c875516a118f5`。

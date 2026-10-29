# 任务025：QueryRegion 双向流

状态：已完成。基准 `a120069096f156b0aa23cf55f9ada0095bcf7087`；批次24及任务1–4证据、三份清单已核验。协议固定 kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认 features。

## 来源与实现

覆盖 [source-inventory.md](source-inventory.md) 任务25行和 [wire-inventory.md](wire-inventory.md) classic PD QueryRegion 行；[surface-inventory.md](surface-inventory.md) 无独立 owner=25 行。routerpb.Router/QueryRegion 属于任务78；HTTP 只观察同监听器 ping，不宣称完成其他 HTTP 路由。Go只读映射为 `server/grpc_service.go:1606–1670`、`pkg/utils/grpcutil/cluster.go:208–224`、`pkg/core/region.go:1627–1768`。主题6准入引用 task-001 历史核验。

[grpc_region_query.rs](../../../pkg/server/grpc_region_query.rs) 在已有 Grpc/SyncGrpc 服务上接入双向流，逐请求执行当前角色和集群校验，并复用现有 `RegionState::query_regions`。通过 [ClusterReadService](../../../pkg/server/lib.rs)、[ClusterHost](../../../pkg/runtime/cluster_host.rs)、[RoleServices/FollowerRead](../../../pkg/runtime/member_role_worker.rs) 读取 canonical leader cache 或已有同步 follower cache；没有新增领域实现、私有 runtime 或平行缓存。

保留 keys/prev_keys 的输入位置、零值 miss、非零缺失 ID 的空 protobuf map value、重复 ID 去重和 peer 信息。每条请求重新读取当前缓存；follower 无需 unary 读取的 opt-in metadata，未同步返回 REGION_NOT_FOUND header 并继续接收；已同步但查询为空仍为成功 header。leader 未 bootstrap 返回 NOT_BOOTSTRAPPED header 并继续；cluster ID 错误结束流。bucket 由请求、leader 身份及既有配置共同控制。输入 EOF 正常结束；响应流直接拥有输入与读取 future，取消、断开和背压无需 detached producer/worker。

## 行为与差分证据

[server 行为测试](../../../pkg/server/grpc_region_query_test.rs) 使用真实 etcd/TCP/tonic，覆盖未 bootstrap、半关闭、同一流读取 heartbeat 更新、查询不改变持久 region entries、HTTP ping 和正常 shutdown。[共享场景](../../../pkg/server/grpc_region_query_scenarios_test.rs) 在一条流内流水线发送6条请求，验证 key/prev-key 顺序、holes、二进制 key、重复 keys/IDs、零 ID、缺失 ID、空请求、bucket 两个开关、错误 cluster ID 的终止顺序、空半关闭、三次客户端取消及后续连接可用。

[runtime 故障测试](../../../pkg/runtime/grpc_region_query_fault_test.rs) 复用已有真实 MetaStore 事务门，验证 canonical heartbeat cache 的非空 report buckets、阻塞真实注册事务时取消等待查询、写入失败后的持久键和缓存均无对应 store。[同步未就绪测试](../../../pkg/runtime/grpc_region_query_readiness_test.rs) 使用真实 RoleServices 和 TCP 服务，在未安装 term/sync worker 时连续查询均返回精确 REGION_NOT_FOUND header，并验证 idle 流服务取消。

[差分测试](../../../pkg/runtime/grpc_region_query_differential_test.rs) 经任务4只读监督入口 `--case region-query` 显式执行，启动真实 Go leader/join follower 和 Rust 双节点。每角色6条完整 protobuf 响应一致（按 protobuf map 语义比较，不依赖 map 编码顺序），仅将已断言正确的 cluster ID 归零；同场景错误文本、半关闭和客户端取消验证均通过。Go oracle 为既有 `bin/pd-server`，build info 为 go1.26.8，无 VCS revision，不声称来自当前 HEAD。非空 bucket、事务等待取消、实时 heartbeat 更新和同步未就绪为 Rust 专项证据，不冒充 Go 故障差分。

初始测试已编译运行，1个真实行为断言因 QueryRegion Unimplemented 失败，退出101。首次故障测试因使用简化 bucket_meta 而非领域 query 消费的 report_buckets 失败；改用真实 report bucket 后通过，未修改领域实现。首次 Go 差分发现测试先等响应头再发送首请求导致等待，发送 SIGTERM 后入口退出130且完整清理；修正客户端先发送首请求后重跑通过。这些中间结果保留在压缩证据。

## 命令与只读守卫

从仓库根动态原子 mkdir 按1–10领取槽位1；所有 Cargo、监督入口和子进程使用 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，无 --target-dir。Rust 修改后执行 `cargo fmt --all`。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_025_ -- --nocapture` | 101 | 1行为失败；非编译错误 |
| 最终同一聚焦命令 | 0 | 1通过、0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_025_ -- --nocapture` | 0 | 2通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 130通过、0忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建，不计行为测试 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case region-query` | 0 | 1通过、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态，不计行为测试 |
| `cargo fmt --all`及`cargo fmt --all -- --check` | 0 | 静态，不计行为测试 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态，不计行为测试 |
| `cargo check --workspace --locked` | 0 | 静态，不计行为测试 |

| `cargo test --workspace --locked -- --test-threads=1` | 0 | 1990通过、20忽略（不计通过） |

Python AST、入口 bash 语法、证据相对链接及 `git diff --check` 通过。没有修改 Cargo.lock 或依赖；编译仅出现既有 cpu_usage deprecated 和共享测试 helper dead_code 警告。

最终监督报告 `compat-readonly-1793264272902401000.json` 的 readonly_unchanged、inputs_unchanged、work_directory_cleaned 和 process_group_cleaned 均为 true。开始/结束 Go保护集合1983文件，含 ignored/untracked，路径、SHA-256和Git状态完全相同；plan.md与基准HEAD完全相同。所有本任务验证与服务进程均已结束；槽位 owner 与锁已释放，缓存保留。其他编号任务的既存删除不纳入提交。

[压缩证据](task-025-evidence.json.gz)包含原始失败/通过日志、命令/退出码/实际测试数量、完整Go前后快照、源码哈希、正常及SIGTERM监督报告。SHA-256：`017270206d20bce070bc9c9ebfbe141ef9f83fb87d1aa981cf23a60579230e63`。

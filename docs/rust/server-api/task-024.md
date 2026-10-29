# 任务024：Region 范围扫描

状态：已完成。基准 `8faa4c4f87cb9c769e84e3ed20649b88ab2cfcb7`；批次23及任务1–4的证据和清单已核验。协议固定 kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认 features。

## 来源与实现

覆盖 [source-inventory.md](source-inventory.md) 任务24行及 [wire-inventory.md](wire-inventory.md) classic PD ScanRegions/BatchScanRegions 两行；[surface-inventory.md](surface-inventory.md) 无独立 owner=24 行。HTTP `/regions/key` 属于任务56，routerpb 独立服务属于任务78。本次 HTTP 观察仅验证同监听器 ping。Go 只读映射为 `server/grpc_service.go:1673–1731`、`pkg/utils/grpcutil/cluster.go:118–219`、`pkg/core/region.go:2120–2240`。主题6准入引用 task-001 历史核验，不作为本次差分。

[grpc_region_scan.rs](../../../pkg/server/grpc_region_scan.rs) 转换协议；[grpc.rs](../../../pkg/server/grpc.rs) 与 [SyncGrpc](../../../pkg/server/sync_transport.rs) 显式转发；[ClusterReadService](../../../pkg/server/lib.rs)、[ClusterHost](../../../pkg/runtime/cluster_host.rs)、[RoleServices/FollowerRead](../../../pkg/runtime/member_role_worker.rs) 接入同一 canonical leader cache 与已有同步 follower cache。没有建立平行服务、缓存或私有 runtime。

扫描与批量合并复用 `RegionsInfo::scan_regions/batch_scan_regions`。领域层只补充 [RegionNotAdjacent](../../../pkg/core/regions_batch_scan.rs) 的前后范围诊断，保留 Go 原始十六进制错误文本；未引入 protobuf 或 transport。请求先校验角色、cluster ID、bootstrap；批量请求在扫描前逐项验证重叠及反向范围。保留非正 limit、半开端点、相邻输入合并、共享 boundary region 去重、总 limit 和 contain_all_key_range 的既有领域行为。

ScanRegions 同时返回 regions、region_metas、leaders；无 leader 时返回空 Peer。BatchScanRegions 按 need_buckets 返回缓存中的 buckets；follower 使用已有仅 region/leader 同步投影。follower metadata 键存在即表示允许读取，即便值为 false；空结果返回 REGION_NOT_FOUND。取消优先于 admission/cache await，缺少配置的 leader 读取服务明确报错。

## 运行证据

[行为测试](../../../pkg/server/grpc_region_scan_test.rs) 使用真实 etcd、双节点 TCP/tonic 和同监听器 HTTP ping；验证 bootstrap 前两种 RPC、两个角色、扫描前后持久 region entries 不变。[共享场景](../../../pkg/server/grpc_region_scan_scenarios_test.rs) 每角色27条响应，覆盖空端点、二进制 key、包含起点的 region、相等端点、holes、limit、相邻与不相邻范围、重复边界 region、空输入、重叠与反向范围、contain_all_key_range、bucket 两个开关和错误 cluster ID。

[故障测试](../../../pkg/runtime/grpc_region_scan_fault_test.rs) 复用已有 Host 与真实 registration 事务 IO 门。先从 canonical heartbeat cache 验证 buckets 的 region ID/version 和请求开关，再阻塞真实写入事务，两种扫描等待相同资源锁；取消 adapter 后两种读取及写入均返回 Unavailable，失败 store 的持久键与缓存都不存在。没有以 mock 调用次数作为验收。

[差分测试](../../../pkg/runtime/grpc_region_scan_differential_test.rs) 通过任务4只读监督入口 `--case region-scan` 显式运行，启动已有 Go leader/join follower 与 Rust 双节点。每角色27条完整 protobuf 字节一致，包含成功结果、错误类型及精确空洞错误文本；只将已断言正确的 cluster ID 归零。Go oracle 为既有 `bin/pd-server`，build info 是 go1.26.8，无 VCS revision，不能声称来自当前 HEAD。取消和非空 bucket 是 Rust 专项证据，不宣称已做 Go 同场景故障差分。

初始聚焦测试已编译并运行，1项实际行为断言因 ScanRegions Unimplemented 失败，退出101。其后实现及共享场景通过；未把编译或零测试当作行为证据。

动态原子 mkdir 按1–10领取槽位1；全部 Cargo、监督入口与子进程使用 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，无 --target-dir。Rust 修改后执行 `cargo fmt --all`。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_024_ -- --nocapture` | 101 | 1行为失败；非编译错误 |
| `cargo fmt --all` | 0 | 格式/构建，不计行为测试 |
| `cargo fmt --all -- --check` | 0 | 格式/构建，不计行为测试 |
| `cargo test --locked -p astersql-pd-server --lib task_024_ -- --nocapture` | 0 | 1通过、0忽略（不计通过） |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 129通过、0忽略（不计通过） |
| `cargo test --locked -p astersql-pd-runtime --lib task_024_ -- --nocapture` | 0 | 1通过、0忽略（不计通过） |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 格式/构建，不计行为测试 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case region-scan` | 0 | 1通过、0忽略（不计通过） |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态/构建，不计行为测试 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 31通过、18忽略（不计通过） |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态/构建，不计行为测试 |
| `cargo check --workspace --locked` | 0 | 静态/构建，不计行为测试 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 1987通过、19忽略（不计通过） |

所有命令从仓库根目录运行。Python AST、入口 bash 语法、相对证据链接、`git diff --check` 通过；未修改 Cargo.lock 或依赖。编译存在既有 cpu_usage deprecated 与共享测试 helper 的 dead_code 警告。

最终监督报告 `compat-readonly-1793263730255174000.json` 的 readonly_unchanged、inputs_unchanged、work_directory_cleaned 及所有命令 process_group_cleaned 均为 true。Go保护集合1983文件，含 ignored/untracked，开始/结束路径、SHA-256与Git状态完全相同；plan.md与基准HEAD完全相同。验证进程已结束，本任务 owner 与槽位锁已释放，编译缓存保留。提交仅包含本任务实现、测试、局部接线和证据；其他编号任务既存删除保持不动。

[压缩证据](task-024-evidence.json.gz)包含原始失败/通过日志、确切命令/退出码、完整Go前后快照、最终源码哈希和监督报告。SHA-256：`d2c1827da6219e45950856fc0d5f3142b279c36a51b789fb85a82e6ca5043a1f`。

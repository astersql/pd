# 任务034：最小 ResolvedTS 上报

状态：已完成。基准 `f6b8872ff51f76c4ff4e855dbd7c40c97662f6c2`；依赖批次33及任务1–4的准入、清单和只读入口已核对。固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认 features，无 Cargo 依赖变更。

## 契约与接线

只读 Go oracle 为 [ReportMinResolvedTS](../../../server/grpc_service.go#L2662)、[SetMinResolvedTS 与聚合](../../../server/cluster/cluster.go#L2624)、[store 选项](../../../pkg/core/store_option.go#L278) 及 [HTTP 观察入口](../../../server/api/min_resolved_ts.go)。Go 每个 store 的报告允许重复、回退、零和 uint64 最大值；单调性属于集群聚合值，不能把单 store 更新改成 max。不存在/已移除 store 返回 Unknown 和完整 PD 错误码文本，离线 store 可以报告但不因此取得 leader。报告只更新内存，不持久化 store 报告；现有聚合 worker 持久化集群最小值。

[RPC适配](../../../pkg/server/grpc_resolved_ts.rs) 复用角色/集群 ID/取消和 bootstrap 校验，返回成功 header 或 NotBootstrapped header。Grpc 和同端口路由实际使用的 SyncGrpc 均注册该方法。[宿主实现](../../../pkg/runtime/cluster_host.rs) 在同一个 resources/state 锁下检查 store 生命周期并 patch canonical StoreInfo，保留心跳和统计字段；Application 与可切换 leader term 的 RoleServices 委托同一宿主，不创建平行缓存、worker、runtime 或持久化格式。角色与存储租约验证沿用已有宿主边界。

来源覆盖：[source-inventory.md](source-inventory.md) 的任务34、grpc_service.go 的 ReportMinResolvedTS、cluster.go 的 SetMinResolvedTS/CheckAndUpdateMinResolvedTS/runMinResolvedTSJob/GetMinResolvedTS 及 min_resolved_ts keypath 行；[wire-inventory.md](wire-inventory.md) 的 ReportMinResolvedTS 行。source 清单把整个 cluster/keypath 目录和共享 persistence/failpoint 行列给多个 owner；本任务复用任务12/16的生命周期和聚合实现，不声称新增了这些目录中所有接口。HTTP min-resolved-ts 路由由任务77负责；[surface-inventory.md](surface-inventory.md) 的配置、审计、限流和指标行继续由47/79–81/90–92验收。本任务 HTTP 观察包括 Rust 同端口 ping 和真实 Go 现有 min-resolved-ts 查询，没有提前接线任务77。

## 当前运行证据

[真实TCP测试](../../../pkg/server/grpc_resolved_ts_test.rs) 覆盖未 bootstrap、错误集群 ID、成功/重复/回退/零/最大值上报、未知/移除/离线 store、同端口 HTTP ping、真实 etcd 聚合持久状态及宿主取消。可控 Tokio 时钟测试驱动真实 ResolvedTsWorker 和 StorageEndpoint/MemoryStore，确认较小值/零/最大值/相等值不会倒退，较大值推进，worker stop 后不再持久化且释放资源。网络测试中的短轮询仅等待真实 RPC/etcd 结果，不用于模拟时钟推进。

[共享场景](../../../pkg/server/grpc_resolved_ts_scenarios_test.rs) 经[差分测试](../../../pkg/runtime/grpc_resolved_ts_differential_test.rs) 与[只读监督入口](../../../scripts/pd-compat-readonly.py)运行真实已有 Go PD 二进制。差分比较未 bootstrap 的 protobuf 错误 header、错误集群 ID 的 FailedPrecondition（先逐端断言完整动态消息）、六次成功响应的完整 protobuf 字节、两个未知 store 和已移除 store 的准确错误码/消息、离线 store 可报告。Rust 从公开 canonical cluster read API 核对每次报告，Go 从现有 HTTP store 查询核对同一数列 `[42,7,0,u64::MAX,100]`；双方最终聚合/持久化为100，Go 重启后仍为100。只归一化已断言语义的集群 ID，不归一化业务时间戳或 store 错误文本。

本 RPC 不执行持久化事务；聚合故障语义复用现有 [task_016_real_storage_failure](../../../server/cluster/resolved_ts_worker_test.rs)，workspace 回归重新运行其真实 etcd 停止/恢复场景：内存先推进、失败不回滚、相同值不重试、后续更大值恢复持久化。本次未注入真实 Go 内部存储故障，不将 Rust 故障测试冒充双端故障差分。

初始测试可编译运行，1项在 Unimplemented 行为断言失败（退出101）。中间两次编译失败揭示 server 自身单测与 runtime dev-dependency 形成两份 server crate 的 trait 身份边界；将 canonical store 的逐值读取放在 runtime 差分中解决，不扩大生产 API。另一次测试在取消后取 node.cluster() 触发已有 leader-term 断言，改为取消前保存宿主引用。上述失败日志保留，不计通过。

## 验证记录

所有阶段从1–10按原子 mkdir 动态领取，实际为槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁记录任务、计划、会话和 PID；Cargo、监督入口及子进程继承绝对路径，无 --target-dir 覆盖。结束后释放自己的锁，保留缓存。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_034_ -- --nocapture` | 101 | 1行为断言失败，非编译错误 |
| 最终同一聚焦命令 | 0 | 2通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 146通过，0忽略 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case report-min-resolved-ts` | 0 | 1真实差分通过，0忽略 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 2019通过（含2文档测试），29忽略，不计通过 |

监督报告 `compat-readonly-1793281698495529000.json`：exit=0，readonly_unchanged、inputs_unchanged、work_directory_cleaned、process_group_cleaned 均为 true。最终验证后再次核对其全部源码和二进制输入哈希，仍一致。已有 Go oracle 为 go1.26.8 构建，固定 kvproto 模块 `v0.0.0-20261016101550-f8ad079c8e98`，SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；无 VCS revision，不声称来自当前 HEAD，完整 build info 保存在监督报告。

开始/结束包含 ignored/untracked 和 target 的1983个 Go/模块文件，存在性、每文件 SHA-256 及受保护 Git 状态完全一致。plan.md 与开始基准逐字节一致。没有执行 Go 测试、make、failpoint 启停、Go 格式化或模块整理；仅复用任务4验证的只读 fixture 构建链和已有 oracle。所有本阶段子进程已结束，自己的锁已释放，编译缓存保留。原有编号任务1–33的删除不纳入本任务提交；任务34按完成要求删除。

Python AST、文档链接和 git diff --check 通过。原始命令输出、失败/通过日志、完整首尾 Go 清单、监督报告、输入哈希及槽位执行器见[压缩证据](task-034-evidence.json.gz)，SHA-256：`15c116eb0eb5fde1a3f2ec70efc180db8f5e4007e2b87449808e6562863aa798`。

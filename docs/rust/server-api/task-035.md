# 任务035：外部时间戳 RPC

状态：已完成。基准 `70c8e32e6`，依赖批次34及任务1–4的清单、领域准入和只读 oracle 入口已核对。协议固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`；默认 features，没有依赖变更。

## 行为与来源

只读 Go 契约：[SetExternalTimestamp/GetExternalTimestamp](../../../server/grpc_service.go#L2698)、[Server 校验](../../../server/server.go#L2531)、[集群内存和持久化顺序](../../../server/cluster/cluster.go#L2744)、[存储编码](../../../pkg/storage/endpoint/external_timestamp.go)、[时间戳比较](../../../pkg/utils/tsoutil/tsoutil.go#L86)。读取未 bootstrap 的集群返回0；写入先分配全局 TSO，超过该值返回 InvalidValue，然后检查 bootstrap，最后要求严格大于当前外部时间戳。相等、回退和0均拒绝。写入先发布内存再保存；存储失败返回 InvalidValue，但不回滚内存值。

[RPC适配](../../../pkg/server/grpc_external_ts.rs) 沿用共享角色、集群 ID 和取消校验。Grpc 与同端口实际路由的 SyncGrpc 都委托该适配。[宿主](../../../pkg/runtime/cluster_host.rs) 复用 canonical StorageEndpoint 和 ClusterPaths，在同一 resources 锁下检查单调性并更新；持久化通过现有 TermStore 做 leader fencing。Application 和 RoleServices 委托同一个宿主，term 启动时恢复已存值；未 bootstrap 的读操作由适配返回0。加载损坏值/后端错误时保留 Go 的零值恢复语义（该恢复分支由源码映射与既有存储测试覆盖，本次没有新增 Go 损坏值差分）。本任务没有新增私有 runtime、worker、存储编码或平行服务。

覆盖 [source-inventory.md](source-inventory.md) 中任务35、grpc_service 的两种 RPC、server 的 Get/SetExternalTS、cluster 的 Get/Set/loadExternalTS、external_timestamp 存储与 keypath 来源；[wire-inventory.md](wire-inventory.md) 的 SetExternalTimestamp/GetExternalTimestamp 行。清单把整个 tso/keypath 目录及共享持久化/failpoint 行分给多个 owner，本任务仅复用现有全局 Oracle、StorageEndpoint 和 leader fencing，不声称完成 MCS/keyspace TSO、ResetTS 或所有路径；这些仍由36/75/76等负责。[surface-inventory.md](surface-inventory.md) 的配置/指标/审计/限流等由原 owner 验收。本任务 HTTP 观察为真实同端口 ping，未扩展 HTTP 产品接口。

## 行为证据

[真实 TCP 测试](../../../pkg/server/grpc_external_ts_test.rs) 验证未 bootstrap 读取、连续成功写入、重复/回退/0/u64::MAX 拒绝、真实 etcd 值、宿主取消及 Rust 重启恢复。初始接口可编译，在 Unimplemented 行为断言失败，1失败、退出101。第一次实现验证因重启使用不同 URL 触发已有 IdentityConflict；保留原 URL 后通过，不更改成员身份规则。

[故障测试](../../../pkg/runtime/grpc_external_ts_fault_test.rs) 仅在外部时间戳事务的真实后端 IO 边界增加超大 mutation，实际 etcd 拒绝整个事务。现有 MetaStore 将其映射为 unexpected metadata backend response，RPC 为 InvalidValue；内存值101、持久值100，重复101拒绝，后续102成功持久化。初始断言误用 etcd 原始错误文本，修正为实际公开映射后重跑。暂停真实事务后取消宿主，RPC终止，持久值仍102，shutdown完成。

[共享场景](../../../pkg/server/grpc_external_ts_scenarios_test.rs) 和 [差分](../../../pkg/runtime/grpc_external_ts_differential_test.rs) 经 [只读监督入口](../../../scripts/pd-compat-readonly.py) 复用既有真实 Go PD 二进制。比较完整 protobuf 响应字节，仅在逐端断言后归一化集群 ID；不归一化业务时间戳或错误消息。包含未 bootstrap Get=0、Set InvalidValue/精确错误文本、两种 RPC 错误集群 ID、成功/重复/回退/0/上限拒绝，最终值102和两端重启恢复。真实 Go 内部存储故障未注入，不把 Rust 故障测试算作双端故障差分。

## 验证记录

所有阶段均按1–10依次原子 mkdir 动态领取，实际槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁记录计划、任务、会话和 PID，Cargo/监督入口/服务子进程继承该绝对路径，无 --target-dir 覆盖。阶段结束释放自己的锁，保留编译缓存。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_035_ -- --nocapture` | 101 | 1行为断言失败；接口可编译 |
| 最终同一聚焦命令 | 0 | 1通过、0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_035_ -- --nocapture` | 0 | 1真实事务故障/取消通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 147通过（146单测、1集成），0忽略 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case external-timestamp` | 0 | 1真实差分通过、0忽略 |

监督报告 `compat-readonly-1793283541042076000.json`：exit=0，readonly_unchanged、inputs_unchanged、work_directory_cleaned 和 commands[0].process_group_cleaned 均 true。首次差分通过 RPC 比较后因工具把 Go ping 的空200 body当 JSON 解析而失败（101）；修正为 curl 检查状态后完整重跑通过，未修改产品 HTTP 行为。

Go oracle 为既有 `bin/pd-server`，go1.26.8 构建，固定 kvproto 模块 `v0.0.0-20261016101550-f8ad079c8e98`；SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。其 build info 无 VCS revision，不声称来自当前 HEAD。完整构建信息与输入哈希在监督报告中。

`cargo test --workspace --locked -- --test-threads=1` 退出0：2021通过（含2项文档编译测试），30忽略，不计通过。最终 `cargo fmt --all -- --check` 退出0。Python AST、本文链接和 `git diff --check` 通过。

开始/结束共1983个 Go/模块文件（含 ignored/untracked）：路径、SHA-256 和 Git 状态完全一致；plan.md 与初始快照逐字节一致。没有执行 make、failpoint 启停、Go测试、Go格式化或模块整理；仅复用经过任务4核验的只读 fixture 构建链和既有 Go oracle。最终监督报告的全部输入哈希再次逐文件校验一致。阶段子进程已全部结束，自己的槽位锁已释放，缓存保留。原有任务1–34文件删除不纳入本次提交；任务35按完成要求删除。

完整初始失败、各次修正和最终通过日志、Go首尾清单、两次监督报告、构建信息和槽位执行器见 [压缩证据](task-035-evidence.json.gz)。SHA-256：`1192d2db98154dc1a566f2f1b05b2e6d2f4b7f89e96f73726f43830e622f5a6d`。

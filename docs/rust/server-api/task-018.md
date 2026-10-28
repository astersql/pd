# 任务018：服务角色请求校验

状态：已完成（本任务公共角色与bootstrap admission边界）。

基准：`817277063dd5962aa477a5527a8dbf86fd814cbf`。依赖任务1–4的清单和只读入口、任务17的已完成证据均已读取；保留编号任务1–17的既有删除。没有修改 plan.md、来源主题、Go 文件或 Go 模块。

## 来源与范围

- [source-inventory.md](source-inventory.md) 任务18行及 `pkg/server/grpc.rs` 的 owner=3,18,25,36 来源行：本任务只覆盖公共 leader-required admission。Go 基准为 `server/grpc_service.go` 的 unaryMiddleware/validateRoleInRequest、StoreHeartbeat、RegionHeartbeat/ReportBuckets；错误码来自 `pkg/errs/errno.go`，bootstrap header 来自 `pkg/utils/grpcutil/cluster.go`。
- [wire-inventory.md](wire-inventory.md) 的 AllocID、GetStore、GetRegionByID、StoreHeartbeat、RegionHeartbeat、ReportBuckets 使用同一现有适配器；完整 RPC 业务仍分别归21/22/23/8/9/10。GetMembers/集群发现归19，Bootstrap/IsBootstrapped归20，TSO完整契约归36，follower region read/转发及HTTP完整契约仍由各方法的owner继续验收。
- [surface-inventory.md](surface-inventory.md) 没有 owner=18 的独立配置、指标或CLI行；本次没有新增配置、指标或命令行选项。只读监督入口增加 `--case request-role` 以运行本任务真实Go差分。

## 实现

[request_role.rs](../../../pkg/server/request_role.rs) 统一停止、宿主当前角色/lease、cluster ID的校验顺序。缺少header与cluster_id=0均按Go GetClusterId处理，不把请求身份错误写成ResponseHeader.Error。停止和角色拒绝为Unavailable，cluster ID错误为FailedPrecondition；不缓存角色结论。

[grpc.rs](../../../pkg/server/grpc.rs) 的已实现leader-required入口使用公共校验。Unary元数据读先校验角色及身份，再返回NOT_BOOTSTRAPPED header。StoreHeartbeat先验证stats存在，再判断bootstrap；缺失stats保持Unknown，并保留原先不回显请求内容的错误文本。RegionHeartbeat和ReportBuckets保留Go的bootstrap优先顺序，错误header属于成功RPC；缺bootstrap时连错误cluster ID也不会进入领域写入。TSO保留源码中独立的identity-before-allocator顺序，没有套用普通unary角色优先规则。

为取得真实未bootstrap证据，局部扩展 [PdServices](../../../pkg/server/lib.rs) 的异步bootstrap状态入口。[RoleServices](../../../pkg/runtime/member_role_worker.rs) 从同一MetaStore通过既有StorageEndpoint.load_meta读取集群元数据；follower没有活动raft-cluster，返回未bootstrap。默认实现保留既有独立适配器宿主的已bootstrap约定。元数据读取错误上送Unavailable，不能误认为已bootstrap。新增引用为Weak，shutdown后不保留MetaStore强引用。没有复制领域实现或引入新的服务/持久化后端。

受影响的既有runtime网络测试显式用StorageEndpoint.save_meta准备真实bootstrap标记；不再依赖过去“选出leader等同bootstrap”的缺口。server单元测试共享一个etcd fixture模块，避免重复include导致同进程相同目录编号互相覆盖。

## 真实行为证据

[request_role_test.rs](../../../pkg/server/request_role_test.rs) 有两项task_018_测试，使用真实etcd、同一MetaStore、PdNodeBuilder及TCP/tonic/HTTP：

1. 两节点leader/follower，覆盖缺失header、显式零cluster ID、合法header。真实follower对普通写请求优先返回Unavailable/not leader，leader错误身份返回FailedPrecondition；无效TSO身份仍优先于其allocator。恢复但未上报的region保持真实调度准备门关闭，排除后台ID预留干扰；拒绝写入前后成功AllocID严格连续。两个listener的HTTP ping均返回200。保持输入端打开的TSO流随shutdown结束，所有Registry清空。
2. 真实store缓存存在但没有持久cluster metadata时，StoreHeartbeat/GetStore和两类心跳流返回NOT_BOOTSTRAPPED；缺stats仍为Unknown。错误cluster ID的unary保持FailedPrecondition。拒绝前后canonical store stats相等，MetaStore没有新增store或cluster元数据。通过既有save_meta提交真实bootstrap标记后，同一listener接纳心跳并更新capacity=180，读取同一store成功。用真实MetaStore事务将marker写为非法protobuf，下一次capacity=999被Unavailable拒绝，canonical capacity保持180；恢复marker并关闭宿主，元数据可重新读取。

初始可编译测试实际运行1项并退出101：`role must be checked before cluster id`，实际FailedPrecondition、预期Unavailable；不是编译错误或零测试。新增bootstrap证据沿用同一真实宿主和存储。

### Go差分

[独立差分](../../../pkg/runtime/request_role_test.rs) 通过任务4只读监督入口启动已有Go PD二进制和第二个真实Go follower；Rust使用两个真实宿主和共享etcd。比较缺失/零/错误cluster ID下AllocID及StoreHeartbeat的实际code/message，合法header下GetStore和StoreHeartbeat的bootstrap header，以及缺失header时RegionHeartbeat/ReportBuckets的bootstrap响应和流EOF；还验证leader能分配ID、follower拒绝合法写请求。

只将独立生成的cluster ID在消息中归一化为CLUSTER，响应header的原始cluster ID先逐个断言正确。所有比较的角色错误、身份错误和bootstrap错误消息/类型保持完整。非法stats的原有请求回显差异不计为本任务完整消息等价；本任务差分的bootstrap写请求使用双方相同的合法stats。

首次差分实际暴露StoreHeartbeat缺stats应先于bootstrap：Rust返回NOT_BOOTSTRAPPED而Go返回Unknown，已按只读Go源码和实际输出纠正顺序。首次runtime回归发现新增强引用延长存储生命周期，已改用Weak，保留原有强引用释放断言。测试还暴露重复fixture目录和后台scheduler的128个ID预留；分别通过共享fixture、真实准备门隔离修正，没有放宽断言或以mock次数验收。

固定kvproto仍为`f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。Go二进制build info没有VCS revision，不声称来自当前HEAD；二进制和实际调用链哈希随监督报告保存。没有Go源码写入或failpoint测试入口；外部etcd不宣称等价于Go embedded-etcd拓扑。

## 验证记录

通过原子mkdir动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。持有进程为本会话shell，记录计划/任务/会话/PID；所有Cargo及子进程继承该目录。最终结果见下方补记；不将编译、零测试或忽略项作为行为成功。

| 确切命令（仓库根目录、默认features） | 退出码 | 实际测试数 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_018_ -- --nocapture` | 101 | 0通过/1失败/0忽略，角色顺序断言 |
| 最终相同聚焦命令 | 0 | 2通过/0失败/0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 119单元+1网络集成通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 28通过/12忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 格式化/静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case request-role` | 0 | 1通过/0忽略 |
| 首轮 `cargo test --workspace --locked` | 101 | runtime中27通过/1端口冲突失败/12忽略；不是成功证据 |
| `cargo test --locked -p astersql-pd-runtime --lib task_006_corrupt_data_releases_resources -- --nocapture` | 0 | 1通过/0忽略 |

首轮workspace失败为既有恢复测试在释放再绑定临时端口时遇到AddrInUse（cluster_restore_test.rs:219），没有修改此断言；同一测试在此前runtime包回归和之后单独复核均通过。为排除测试间临时端口竞争，最终使用全workspace串行测试，结果见末尾。中途修正fixture隔离前的诊断运行和一次主动中断的workspace编译不作为通过证据；完整日志均保留。


## 最终核验

- `cargo test --workspace --locked -- --test-threads=1`：退出0，1973项普通测试和2项doc-tests通过，共1975通过；13项忽略不计通过。本任务Go差分另由监督入口显式运行1项、0忽略并通过。
- 最终监督报告 `compat-readonly-1793167982445332000.json`：exit=0，readonly_unchanged、inputs_unchanged、process_group_cleaned、work_directory_cleaned均为true；完整记录保存在[压缩证据](task-018-evidence.json.gz)。
- 开始/结束Go/模块集合1983文件（含ignored/untracked），路径与原字节SHA-256完全一致；排序JSON集合摘要`300a4b0fdad52dd6b67f5daa919a1720ca47c97ecc7a81d364f6896f12d094a2`，Go/模块Git状态均无差异。plan.md与HEAD原字节一致，验证期间实现及测试源码哈希没有漂移。
- 所有Cargo、fixture及监督子进程已结束。自己的slot-1.lock已通过shell EXIT清理，编译缓存保留；没有本任务遗留的etcd-contract数据目录；保留原有Oct26/27创建的`etcd-contract-35880-1`和`etcd-contract-23371-3`，其持有进程均已不存在，未删除其他任务缓存。Python AST、Bash语法、任务文档链接和git diff --check均通过。
- 压缩证据SHA-256：`cc1698086af6c295ed0265f571bcd9f05671568f36bebcb4238ee4a4e972bede`。重复Go守卫快照用snapshot_ref引用guard_snapshots，原始快照及完整命令日志保留。仅提交本任务实现、测试、文档和证据；完成后删除编号任务18，保留既有编号任务1–17删除。

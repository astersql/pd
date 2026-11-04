# 控制面与 MCS 实施准入

任务 001，2026-11-04，基准 `edc0ef5c83398825c7cdfa2c5cf6a341fe677c73`。
结论：允许按依赖顺序实施本计划；不授予领域、独立 MCS 或生产后端兼容认证。
本任务核对的是可复用公共接口及其证据，不要求服务端任务102先整体完成。
因此没有“102等主题8、主题8等102”的实施依赖环。

## 公共接口及准入边界

| 能力 | 当前公开入口和实际调用方 | 已有验证入口 | 结论与限制 |
| --- | --- | --- | --- |
| 协议 | [proto build](../../../pkg/proto/build.rs)，[server](../../../pkg/server/grpc.rs)消费生成的 pdpb；同时生成 keyspacepb、tsopb、meta_storagepb、routerpb、resource_manager、schedulingpb | [task100](../server-api/task-100.md)、[协议回归](../../../tests/rust-compat/grpc_regression_test.rs) | 固定 f8ad079；41个 classic 方法有既有证据，15个 GC 方法待主题8。生成绑定不等于注册服务 |
| 领域存储 | [MetaStore](../../../pkg/storage/kv/meta_store.rs)：read、transact、transact_branches、transact_raw、watch、lease、compact；[domain](../../../pkg/storage/domain/kv.rs)为 canonical 契约，[etcd adapter](../../../pkg/storage/kv/etcd_store.rs)实现；[runtime node](../../../pkg/runtime/node.rs)传入同一 store | [契约](../../../pkg/storage/kv/contract_test.rs)、[etcd集成](../../../pkg/storage/kv/etcd_store_test.rs)、[取消](../../../pkg/storage/kv/txn_cancel_test.rs) | 可用于新领域。Unavailable的写入结果可能未知，禁止无条件重试。MemoryStore不能证明磁盘耐久性 |
| 领域状态存储 | [domain StorageEndpoint](../../../pkg/storage/domain/endpoint.rs)、[endpoint facade](../../../pkg/storage/endpoint/lib.rs)：配置、region、GC状态/屏障、keyspace元数据及分组读写；[GC读测试](../../../pkg/storage/endpoint/gc_safepoint_read_test.rs)、[事务测试](../../../pkg/storage/endpoint/gc_state_txn_test.rs) | 对应独立 endpoint 测试与 task102既有工作区回归 | 已有持久原语可复用；GC/键空间管理规则仍须在canonical领域crate实现，不在transport复制 |
| 选主 | [Member](../../../pkg/member/member.rs)::bootstrap/campaign；[Leadership](../../../pkg/election/leadership.rs)::campaign_with_key、validate、transact、renew、resign；[runtime](../../../pkg/runtime/node.rs)拥有任期及取消 | [lease](../../../pkg/election/lease_keep_test.rs)、[leader watch](../../../pkg/election/leader_watch_test.rs)、[成员角色](../../../pkg/runtime/member_role_worker_test.rs) | 复用lease/value fencing；MCS专属primary键、租约发布与发现尚待58–64及服务任务 |
| TSO | [Oracle](../../../pkg/tso/oracle.rs)::new、allocate、reset、update，Clock接口；[runtime tso](../../../pkg/runtime/tso.rs)接线 | [oracle](../../../pkg/tso/oracle_test.rs)、[restart](../../../pkg/tso/restart_test.rs)、[差分](../../../pkg/runtime/grpc_tso_differential_test.rs) | classic单组已实现；MCS分组及跨组语义不由本项准入 |
| 宿主生命周期 | [PdNodeBuilder/PdNode](../../../pkg/runtime/node.rs)：new接收Handle/CancellationToken，start接收store/listener/Registry，ready、wait、shutdown；[CLI](../../../cmd/pd-server/main.rs)拥有进程资源 | [node](../../../pkg/runtime/node_test.rs)、[shutdown](../../../tests/rust-compat/rolling_restart_test.rs)、[task102](../server-api/task-102.md) | 可复用宿主模式；库不新建私有runtime、不装全局信号。独立/嵌入MCS由62–64、131–139验收 |
| 调度 | [Coordinator](../../../pkg/schedule/coordinator.rs)复用BasicCluster、checker/operator/scheduler；[ClusterHost](../../../pkg/runtime/cluster_host.rs)拥有watch_schedule、snapshot及worker状态 | [主题6最终证据](../schedule-execution/task-156.md)、[runtime worker](../../../pkg/runtime/schedule_worker_test.rs) | 可复用现有调度状态；独立Scheduling服务、watch恢复与跨服务调用尚待111–130 |
| HTTP与指标 | [PdServices](../../../pkg/server/lib.rs)隔离领域接口；[v2边界](../../../pkg/server/http_v2_boundary.rs)明确依赖拒绝；[task101](../server-api/task-101.md)保留领域依赖 | [v2行为](../../../pkg/server/http_v2_boundary_test.rs)、[来源审计](../../../tests/rust-compat/workspace_acceptance_test.rs) | 当前26个领域路由503不是兼容通过。已有宿主Registry可复用；不提前声称GC/MCS指标完成 |

上表入口、调用方和测试均为实际存在源码。核对实现不等于重跑全部历史测试。
任务102的正式证据记录历史完整workspace 2252通过和四crate联合468通过，
均0失败、82默认忽略；这里只引用其明确范围，不改变它们的时间或忽略口径。
本聊天前一轮在同一基准实跑v2边界2通过、1忽略和来源审计2通过；
任务001本身纯文档核对，新增行为测试0、差分0，不运行Cargo。

## 实际crate与后续责任

扫描根及子manifest共75个package；主题8源码扫描163个文件，
完整路径与manifest哈希见[只读盘点记录](task-001-evidence.json.gz)。

| 现有范围 | 真实Rust入口 | 能力归属和仍需完成的任务 |
| --- | --- | --- |
| 加密 | [astersql-pd-encryption](../../../pkg/encryption/Cargo.toml)，[Config](../../../pkg/encryption/lib.rs)，[测试](../../../pkg/encryption/config_test.rs) | 仅配置及master key描述；9–15负责key IO、加解密、KMS及生命周期，134接线 |
| 键空间常量 | [astersql-pd-keyspace-constant](../../../pkg/keyspace/constant/Cargo.toml)，[常量](../../../pkg/keyspace/constant/lib.rs) | 不存在keyspace管理crate；25–42及131负责范围/分组/创建/配置/状态/删除/接线 |
| MCS常量 | [astersql-pd-mcs-constant](../../../pkg/mcs/utils/constant/Cargo.toml)，[常量](../../../pkg/mcs/utils/constant/lib.rs) | 54–64负责工具/registry/discovery/server；各独立服务另行验收 |
| ResourceManager配置 | [astersql-pd-rm-server](../../../pkg/mcs/resourcemanager/server/Cargo.toml)，[配置](../../../pkg/mcs/resourcemanager/server/lib.rs) | 仅配置；89–110领域/协议与138嵌入装配未认证 |
| replication / unsaferecovery / GC / tsoutil | 对应目录只有Go oracle，无Rust manifest | 4–8、16–24、43–53、69–71建立真实canonical行为，133/135/132装配 |
| MetaStorage / TSO / Router / Scheduling MCS | 无服务Rust manifest；现有classic TSO和调度不可视为对应MCS已完成 | 65–88、111–130及136/137/139负责；140–149负责跨服务和最终验收 |

未实现领域属于本计划要修复的对象，不是任务001公共接口准入失败。
每个行为任务仍须核对其直接接口及oracle可达性；本表不豁免真实成功、错误、取消、
持久状态及故障证据，也不允许用默认tonic桩或固定HTTP响应销项。
主题10生产后端仍独立按候选实验/ADR/契约计划推进；外部etcd仅为过渡模式，
不声称Go embedded-etcd拓扑或原生durability已等价。

## 只读差分环境

现有Go oracle：`/tmp/pd-task1-go-env/pd-server-without-dashboard-amd64`。
通过已有SDK `/tmp/pd-task1-go-env/go/bin/go version -m` 读取构建信息（退出0）：
`kvproto=v0.0.0-20261016101550-f8ad079c8e98`、`GOARCH=amd64`、
`-tags=without_dashboard`、`vcs.revision=b53bc9a015943526b5e61c6ae9e2ea1b87856972`。
精确二进制SHA-256、完整build info、现有Rust二进制及etcd/protoc路径和哈希
保存在盘点记录的 `binaries`；使用前须重新比对，不能只信文件名。
amd64数值转换合同和without_dashboard范围沿用task102既有认证，不扩张为其他平台。

协议ZIP缓存：`GOMODCACHE=/tmp/pd-task1-go-env/mod`，
版本和ZIP摘要由proto build固定为
`aff09d074d493874bfd7e3e63c766a8d93f42901ff235dde2d2ced0c7f2a615e`。
显式设置GOMODCACHE后构建只读ZIP且仅提取proto；不下载或生成Go文件。
PROTOC使用盘点记录中的现有macOS arm64 vendored二进制。
真实etcd fixture须显式设置PD_COMPAT_ETCD_BIN为现有官方3.5.15二进制，
避免fallback构建Go fixture；CLI supervisor同时设置相邻PD_ETCD_BIN/etcdctl。
Go SDK仅供只读版本检查，PATH加入现有SDK的bin目录；不运行Go构建、测试、make、
failpoint注入或模块整理。

复用[只读supervisor](../../../scripts/pd-compat-readonly.py)及
[shell入口](../../../scripts/run-pd-compat-readonly.sh)，每次先原子领取共享槽位，
设置绝对CARGO_TARGET_DIR、PD_COMPAT_MODE=dual、上述Go二进制及该槽位debug/pd-server，
要求 `--require-pinned-protocol`。脚本检查不同二进制、非空fixture、协议build info、
输入哈希、Go文件快照和进程清理；任务003扩展领域场景与负向控制。
不存在的二进制、错协议、输入漂移或未提供领域场景必须失败，不能静默skip。

## 本任务验证

- `git rev-parse HEAD`、`git status --short --branch`退出0；保留既有102任务修改和
  2026-11-02六项收尾计划目录的8个文件删除；开始时无暂存内容。
- `rg --files pkg/mcs pkg/keyspace pkg/gc pkg/replication pkg/encryption pkg/unsaferecovery pkg/utils/tsoutil tests/integrations/mcs`
  退出0；163个文件及75个manifest的真实名称和摘要保存于盘点记录。
- 开始和结束Go源码/模块（含ignored/untracked）1101项的存在性、SHA-256及Git状态
  完全一致；快照存于盘点记录的go_readonly。所有受保护路径Git状态为空。
- 本文所有相对文件链接均检查存在；`git diff --check`退出0。
- 未运行Cargo及Go测试；有效新增行为测试0。无需Cargo槽位。

任务001的文档盘点与公共接口准入完成，后续按任务002固定逐行覆盖，
再由任务003建立可拒绝错误输入的领域差分入口。

# 标识分配契约盘点

## 任务 1：盘点分配器差距（批次 70）

日期：2026-10-23（Asia/Shanghai）。状态：已完成。仅新增事实清单，不改变运行行为。
规划基准：`5d5948052a9ee9e3fa7a09145fe2916a2912c223`；本轮审查 HEAD：
`e28db03b82eee662b55c1c852f7a3d21842cd7ad`。以下符号、行号及内容摘要固定于此 HEAD。
已阅读 [仓库指南](../../AGENTS.md)、[Copilot 指南](../../.github/copilot-instructions.md)、
[ADR-001](architecture.md)、[移植指南](porting-guide.md)、[依赖基线](dependencies.md)、
本子计划只读 plan.md 与来源主题 3。仓库只有根 AGENTS.md，无下层覆盖。

### 准入与证据边界

依赖批次 69 已满足：提交 `e28db03b8` 包含
[leader_priority.rs](../../pkg/member/leader_priority.rs)、
[leader_priority_test.rs](../../pkg/member/leader_priority_test.rs)、
[显式注册](../../pkg/member/lib.rs) 及 fixture 模块复用修复。
[成员契约证据任务 3](theme3-成员契约补齐.md) 记载红灯 1 passed / 2 failed，
绿灯 3 passed；修复 fixture 注册冲突后完整 workspace 回归退出 0，
636 passed / 0 failed / 1 既有 ignored。此处核验的是前序已保存证据与真实提交，
没有重新运行 Cargo，也没有以任务文件删除推断完成。

本轮只读穷举源码、既有测试及装配，执行文件/链接/差异检查。
所有行为举例均为源码推导，既有测试均只读核验断言，不能称为本轮运行差分。
未运行 Go、Cargo、make、failpoint 或生成器；不需要 Cargo 槽位。
Go 两个主来源相对规划基准无变化；allocator.rs 唯一差异为 MetaStore import
由 storage 改至 canonical storage-kv，未新增 API。

### Go 公开符号与内部行为全覆盖

主来源：[id.go](../../pkg/id/id.go)、[id_test.go](../../pkg/id/id_test.go)。
“任务 2/3/4”分别指本计划批次 71/72/73；上层任务号指来源渐进迁移主题。

| 符号（id.go 行号） | 当前契约、Rust 对照与归属 |
| --- | --- |
| `label` :36 | 私有 string 类型；不是任意持久命名空间。仅精确等于 KeyspaceLabel 时切换键及上界。任务 4。 |
| `DefaultLabel` :40 | `idalloc`，普通全局 ID；Rust 只有这一范围。任务 4 保持既有构造入口。 |
| `KeyspaceLabel` :42 | `keyspace-idAlloc`，集群级 keyspace ID 分配器；不是每个 keyspace 各自一套分配器。任务 4。 |
| `DefaultAllocStep` :45 | u64 1000；构造参数 Step=0 使用它。Rust DEFAULT_ALLOC_STEP/new 已有；任务 2/4 保留。 |
| `Allocator` :49 | 接口完整包含 SetBase、Alloc、Rebase；Rust 同名具体 struct 仅有 new/alloc。任务 2/3/4。 |
| `AllocatorParams` :80 | Client、Label、Member、Step；Client 是具体 etcd client，Member 为 leader value。Rust 接受唯一 MetaStore、Arc Leadership、宿主取消令牌、step，不迁移具体 client API。任务 4 仅扩展必要 namespace。 |
| `NewAllocator` :88 | 不读写存储，初始 base=end=0；默认上界 MaxUint64，KeyspaceLabel 依 kernel 模式选择上界。Rust new 校验同一个 Arc store，固定全局键、无 effectiveEnd。任务 4。 |
| `Alloc(count uint32) (uint64,uint32,error)` :113 | 返回最后一个 ID、原 count、错误；整个循环持锁，每个 ID 都检查上界并按需预留窗口。Rust alloc() 只返回一个 u64。任务 2。 |
| `SetBase(newBase uint64) error` :136 | 拒绝 newBase>=effectiveEnd；否则先改本地 end，再以 checkCurrEnd=false 预留。不检查旧 base、旧持久值或单调性。Rust 缺失。任务 3；详见下述契约冲突。 |
| `Rebase() error` :153 | 持锁调用 rebaseLocked(true)，主动丢弃剩余本地窗口并从持久边界预留。Rust 只有内部耗尽时预留，没有公开 rebase。作为任务 3 的窗口调整相关缺口记录；若仅实现 SetBase，公开 Rebase 仍属上层主题 3 ID 契约验收，不可宣称整包完成。 |
| `allocatorImpl` :61、`metrics` :75 | 私有锁、base/end、client/label/member/step/metrics/effectiveEnd 与 gauge。Rust Window(next,end)、Mutex、store/leader/cancel/key/step 已有。领域状态随任务 2/3/4；指标与恢复日志兼容归上层主题 3/9，不在此计划加指标。 |
| `rebaseLocked(checkCurrEnd bool)` :160 | 私有事务算法、编码、错误及失败状态见下表；不是遗漏的公开 API。任务 2/3/4 复用既有 allocator，不建第二套实现。 |

### count、窗口与失败的精确边界

- count=0：循环零次，返回当前 base、0、nil；全新实例即 (0,0,nil)。不触达存储，
  不检查耗尽或 leader。Go gRPC 把请求 count=0 改成 1，这是适配层行为，不能混入领域 API。
- count=1：先判断 base>=effectiveEnd，命中则 (0,0,ErrIDExhausted)；
  base==end 时重新预留，随后 base++。成功返回新 base、1、nil。
- count>1：逐个消费而非一次性加 count。例 step=3、初始空存储、Alloc(5)
  推导为持久边界 6、本地 base=5/end=6、返回 (5,5,nil)，消费 ID 1..5。
  不返回起点、数组或半开区间。单实例跨窗口连续；多个实例的独立窗口可以形成间隙。
- 批次中途耗尽或事务失败：返回 (0,0,error)，此前 base 增量及已提交预留没有回滚。
  任务 2 必须覆盖 count=0、跨窗、并发、旧 term、边界及部分消费后失败，
  不能把请求解释为全有或全无事务。
- Go `end+step` 用 u64 算术，可能先溢出，`base=end-step` 也可能回绕。
  Rust 已用 saturating_add、保留旧 base，已有最后一个 u64 与耗尽契约；
  任务 2/4 不得机械复制溢出破坏既有唯一性/耗尽证据。

### SetBase 的契约冲突与失败状态

Go 只拒绝上界，不拒绝下降。源码推导：全局 step=10，空存储先 Alloc(1) 返回 1，再 SetBase(100)
写 110、设 base=100，下次 Alloc(1) 返回 101；随后 SetBase(0) 写 10、设 base=0，
下次返回 1，可以重用既有 ID。也没有读取持久值进行 CAS，只有 leader value 条件。
这与任务 3 的“不允许重用已经分配的 ID”目的冲突；其“精确按 Go 规则处理拒绝或忽略”
不能推导出 Go 会拒绝或忽略降低操作。**批次 72 实施前必须明确安全强化与 Go 差异的裁决，
不得把拒绝下降声称为 Go 现状。** 本轮只完成盘点，不擅改计划或实现。

另一个边界：SetBase 预先写本地 end，事务失败不会还原，base 保持原值；
成功时 base=end-step。Keyspace 接近上界且 step>1 时，截断 end 后 base 可能小于 newBase，
甚至重复之前 ID；现有 Go 测试仅要求非递减，未断言严格唯一。
任务 3 负责提升、降低、跨窗、冲突及重建的取舍证据；任务 4 负责 effectiveEnd
及普通/NextGen 界限，不可用安全目标掩盖这些 Go 源码事实。

### key/value、事务、watch 与 lease

| 对象 | Go 事实 | Rust 当前事实与归属 |
| --- | --- | --- |
| 全局持久键 | `/pd/{cluster_id}/alloc_id`，AllocIDPath | new 内按显式 leader.cluster_id 拼接相同键；已实现。任务 4 保持。 |
| keyspace 持久键 | `/pd/{cluster_id}/keyspaces/alloc_id`，KeyspaceAllocIDPath | ClusterPaths 两个路径函数已有，allocator 尚未使用 keyspace 键。任务 4。 |
| leader 键 | ElectionPath(nil)，PD `/pd/{cluster_id}/leader` | canonical Leadership；无全局 cluster ID。任务 2/3/4 保持。 |
| 值 | Uint64ToBytes/BytesToUint64：恰好 8 字节大端 u64，非 8 字节错误；不存在从 0 开始预留 | to_be_bytes/from_be_bytes；错误 CorruptBoundary；prefix read 后 exact key 过滤。既有能力。 |
| 普通重建/续窗 | 先读旧值；缺失比较 create_revision=0，存在比较 value=原始字节；所有分支比较 leader value；Put 新 end | 线性读后用 mod_revision/create_revision CAS；通过 Leadership::transact 同时比较 leader value、lease、mod_revision；比较冲突 yield 后重读重试。保持现有强化语义。 |
| SetBase 写 | 不读旧边界，不作 allocator CAS；仅 leader value 条件 Put | 未实现。任务 3 负责冲突及不重用要求，并显式记录 Go 差异。 |
| 错误 | 读/解码错误上返；提交失败 ErrEtcdTxnInternal；条件失败 ErrEtcdTxnConflict；耗尽 ErrIDExhausted；不自行重试 | Store、Member、Cancelled、Exhausted、CorruptBoundary、StoreMismatch；传输失败直接上返，禁止不确定提交盲目重试。既有错误不得被 count/SetBase 路径吞掉。 |
| watch | allocator 本身无 watch | allocator 本身无 watch；选主观察在 election，不增设 ID watch。 |
| lease | ID 边界 Put 不附 lease；Go 缓存消费无额外 leader 校验 | ID 边界仍无 lease；每次 alloc 在消费缓存前 validate，预留事务再 fencing。任务 2/3/4 复用，不弱化。 |
| 上界 | 全局 MaxUint64；普通 keyspace 0xFFFFFF=16777215；NextGen ReservedKeyspaceIDStart-1=16776190 | 只有全局 u64 上界；任务 4 补两个 keyspace 上界，模式由显式领域输入提供，不引入全局环境。 |

键路径依据 [absolute_key_path.go](../../pkg/utils/keypath/absolute_key_path.go):40–41、154–160，
已有 Rust [cluster_paths.rs](../../pkg/utils/keypath/cluster_paths.rs):50–55；
编码依据 [conversion.go](../../pkg/utils/typeutil/conversion.go):23–37；
上界依据 [constant.go](../../pkg/keyspace/constant/constant.go)；
fencing 依据 [leadership.rs](../../pkg/election/leadership.rs):159–199。
上述字节格式为源码事实，本轮未获得新的 Go 编码运行输出，未自制 oracle。
未来编码任务如不能复用合格已有输出或只读工具，须按其任务规则阻塞。

### Rust 公开符号、注册及现有断言

[allocator.rs](../../pkg/id/allocator.rs) 全部公开 API：
DEFAULT_ALLOC_STEP；AllocatorError（Store、Member、Cancelled、Exhausted、
CorruptBoundary、StoreMismatch）；Allocator；new(store,leader,cancel,step)；
alloc(&self)->Result<u64,AllocatorError>。多行 new 签名已完整阅读。
私有 Window、allocate 也已逐行核验；无 count、SetBase、Rebase 或 namespace 构造 API。
[lib.rs](../../pkg/id/lib.rs) 重导出 allocator 并注册 allocator_test；
[Cargo.toml](../../pkg/id/Cargo.toml) 显式注册 etcd_behavior target。
依赖仅 storage-kv/member/thiserror/tokio/tokio-util，无协议类型或私有 runtime。

| 既有测试/帮助函数 | 只读核验到的断言及覆盖缺口 |
| --- | --- |
| Go TestMain | goleak + WaitForEtcdConnections；非业务 oracle。 |
| Go TestMultipleAllocator / testAllocator | DefaultLabel(step=0=>1000) 与 KeyspaceLabel(step=500) 各 goroutine，反复单 ID 检查递增；不是同命名空间多实例竞争测试，不覆盖 count>1。 |
| Go TestIDAllocationEndValue / checkIDAllocationEndValue | 根据编译模式选上界，step=1/10/1024/1025，SetBase(end-step*3) 后分配至错误；断言非递减、最后等于上界；SetBase(end-1) 成功、end/end+10 报耗尽。未覆盖降低、失败后本地状态、批次部分消费。 |
| Go tests/server/id TestBatchAllocID / checkBatchAllocID | 10 goroutine 各 200 次 Alloc(10)，以返回 id/count 检查不重复；测试从 id-count 开始枚举，较实际消费区间偏一，不能据此重定义返回起点。见真实 Alloc 循环。 |
| Go tests/server/id TestID、TestCommand、TestPDRestart、TestMonotonicID | 提供单 ID、gRPC 单 ID 请求、重启、选主相邻场景；含 failpoint，未运行，服务装配归上层主题 7。 |
| Rust allocator_test::behavior / memory_concurrency_restart_fencing | 两实例 step=7，16 workers 各 20 次，共 320 个唯一值；重建超过持久边界；撤销旧 term 拒绝缓存分配，重选后继续，取消拒绝。无 count/namespace/SetBase。 |
| Rust allocator_test::boundaries / memory_corrupt_and_exhausted_boundary | 非 8 字节拒绝；预置 MAX-1 后分配 MAX，再耗尽；不同 Arc store 拒绝。 |
| Rust real_etcd_behavior / real_etcd_boundaries | 调用同一 behavior/boundaries，复用 canonical KV fixture；不是 mock。 |
| Rust real_etcd_wal_restart | 同 WAL stop/start，首次 1、预留 1000，重启后 1001；无 SetBase/keyspace。 |

Rust 测试来源：[allocator_test.rs](../../pkg/id/allocator_test.rs)、
[etcd_behavior_test.rs](../../pkg/id/etcd_behavior_test.rs)。后者通过 path 包含前者，
其中两个 memory 测试也会被编译进入集成 target；不能将注册数当作独立行为覆盖数。
前序 [member-id 基线](member-id.md) 的不确定提交规则有源码依据（不重试 Store 错误），
当前这些 ID 测试没有“提交成功但响应丢失”的专门故障断言，不将该项说成新获运行证据；
属来源主题 3 的故障闭环，当前任务 2/3/4 必须保持既有处理。

### 反向依赖与剩余归属

- Go 生产装配：[server.go](../../server/server.go):506/537 分别构造全局/keyspace allocator，
  2134 在 leader 流程 Rebase，2528 包装 SetBase；
  [grpc_service.go](../../server/grpc_service.go):702–744 将 count=0 归一为 1，
  成功时仅 count>1 才填响应 Count，错误包装到 Header。
  allocator 领域 count 归任务 2，服务协议/启动/恢复命令归上层主题 7。
- Go [cluster.go](../../server/cluster/cluster.go) 与
  [cluster_worker.go](../../server/cluster/cluster_worker.go) 为 region/peer 分配单 ID；
  [keyspace.go](../../pkg/keyspace/keyspace.go):1448 分配 keyspace ID。
  前者业务算法归上层主题 6/7，后者业务管理归上层主题 8；此计划仅补底层 namespace。
- Go 导入 `pkg/id` 的完整文件集合还包括 tests/cluster.go、
  server/cluster/cluster_test.go、tests/server/cluster/cluster_test.go、
  pkg/gc/gc_state_manager_test.go、tools/pd-ctl/tests/keyspace/keyspace_group_test.go，
  以及上文已列文件。tests/testutil.go、tests/integrations/client/client_test.go、
  tests/integrations/mcs/tso/keyspace_group_manager_test.go 等通过持有的 allocator 间接调用，
  分别属于测试工具/客户端/MCS 上层主题，不将 tempurl.Alloc 或 TSO allocator 算成 ID API。
- Rust 生产直接消费者仅 [runtime/node.rs](../../pkg/runtime/node.rs):86/146，
  工厂传 step=0，Application.alloc_id 只支持单值；
  [server/lib.rs](../../pkg/server/lib.rs) 的 PdServices 及
  [server/grpc.rs](../../pkg/server/grpc.rs):54–77 当前拒绝 count>1。
  这是上层服务主题 7 的真实未覆盖项；批次 71 仅补领域 count 不会自动使 wire API 对齐。
  [pkg/lib.rs](../../pkg/lib.rs) 根 facade 只重导出 runtime API，不直接重导出 ID crate 或复制 allocator。
- [metrics.go](../../pkg/id/metrics.go) 的 init 注册 pd_cluster_id GaugeVec(type)，
  成功预留时设 end；恢复日志保留 new-end 为首字段。Rust ID 当前无这些指标/日志。
  归上层主题 3/9 的监控与恢复兼容，不在当前补 API 计划引入指标依赖。

### 后续文件与验收归属

任务 2 在 canonical allocator.rs 原位扩展 count；任务 3 处理 SetBase/边界调整；
任务 4 处理 namespace 与有效上界。以下均为编号计划中的**拟建路径**，当前不存在，
不作为已实现或已通过依据；优先按任务允许范围扩展既有 canonical 模块：

```text
pkg/id/allocate_count.rs / pkg/id/allocate_count_test.rs     # 拟建，任务 2
pkg/id/allocator_base.rs / pkg/id/allocator_base_test.rs       # 拟建，任务 3
pkg/id/allocator_namespace.rs / pkg/id/allocator_namespace_test.rs # 拟建，任务 4
```

本轮未出现未归属缺口。SetBase 语义裁决与未来必要运行/编码证据仍是对应实现任务的
准入/验收责任，本盘点完成不表示批次 71–73 或来源主题 3 已完成。

### 本轮验证

| 命令/检查 | 结果 |
| --- | --- |
| `git rev-parse HEAD`、`git show --stat e28db03b8`、前序证据逐项核验 | 0；批次 69 产物与已保存回归齐全。 |
| `rg -n '^(func |type |pub |    pub )' pkg/id/id.go pkg/id/id_test.go pkg/id/allocator.rs` | 0；20 条匹配；补读常量、接口方法、多行签名、私有路径及所有测试断言。 |
| `git diff 5d5948052a9ee9e3fa7a09145fe2916a2912c223 HEAD -- pkg/id/id.go pkg/id/id_test.go pkg/id/allocator.rs` | 0；Go 无差异，Rust 仅 canonical kv import 归属变化。 |
| 反向依赖 `rg` 搜索、源码/装配人工逐项核验 | 0；领域与服务 count、SetBase 差异及 namespace 边界均明确归属。 |
| Markdown 相对链接和源文件 SHA-256 检查 | 0；实际链接均存在，拟建文件单独标注。 |
| Go/模块开始结束路径与 SHA-256、Git 状态对照 | 0；1101 个文件完全一致，相关 Git 状态为空。 |
| `git diff --check`、`git diff --cached --check` | 0；本任务仅文档，编号任务完成后删除；已有其他任务变更排除提交。 |

没有执行运行行为测试，TDD 不适用；符合本任务文档验证要求。未修改只读总计划、
来源主题、Go/模块、Rust/Cargo 文件；本次审查差异仅证据文档及本编号任务状态/删除。

### 固定来源摘要

下表包括本轮事实清单引用的全部实现、测试、装配和 manifest，采用 SHA-256；
规则与前序证据由上述 HEAD 和显式链接固定。

| 来源路径 | SHA-256 |
| --- | --- |
| `pkg/election/leadership.rs` | `9275a49e6cf76f9560a4b826a30beaacc1f2ea4229948df97632648236f4ff2c` |
| `pkg/gc/gc_state_manager_test.go` | `85bd4b1726ac87f17728fec5b8a280cc08ec18bd396dfb4231220072b38583ba` |
| `pkg/id/Cargo.toml` | `317575319898f2a13ffba5b298f3aa859be92cfe0bb39d585b2e7daa35954ac3` |
| `pkg/id/allocator.rs` | `6918ca3a8784de53696d8e2b200632ced335f44a8fa7c933c5675f42e15b2052` |
| `pkg/id/allocator_test.rs` | `e89e5dd14de594ee51b75c378cff5aa3863c982ba42feb6c36ac53e2b2e647de` |
| `pkg/id/etcd_behavior_test.rs` | `b28e9ac1cbd915b7fc2e5e373ae58a71eedfc9bcbf3a4795886844ae206230c1` |
| `pkg/id/id.go` | `ce6835b3aba6b281e44197ef36b17a20c9e2e7857d30d4cd999e26a15cd01c84` |
| `pkg/id/id_test.go` | `1fe8e34a2e5dfce53e9ce008754a4b9c32d590992fc7d105eccf9d70edf7a9ed` |
| `pkg/id/lib.rs` | `ccd1b035fb1f848f00f0bfa81faae1a3315900cc9b7a6957cb6913dd412e859a` |
| `pkg/id/metrics.go` | `613224400397f0de556f37895880861d610afd605507f2403dbd34fa10d8065b` |
| `pkg/keyspace/constant/constant.go` | `c9489b63673dff2185ec34b71d44b791c29798abe7e7fb959dc53c2a13049921` |
| `pkg/keyspace/keyspace.go` | `545c3d7c184e18a52dd3e4aad16a7b1ae65ba379cb25746899562bf7fc280795` |
| `pkg/lib.rs` | `31755c4c6a2a500cd54edf42614edb9991c14a1f38c89ac622c5b29ce13b454b` |
| `pkg/member/leader_priority.rs` | `2612d040262b85d591082ca6ecc1e6b4a202d221178ee4df1c9b4803ed9acb0d` |
| `pkg/member/leader_priority_test.rs` | `57ccb8260ac347a7102686e882c70867835539c8764ca16cb1762dbeee81c533` |
| `pkg/member/lib.rs` | `448ca6c3fd0feb9121252042d7cf986b677453ad87cf17a4ac16c4ae8fae1bf8` |
| `pkg/runtime/node.rs` | `aaaf21d8c9010cecae983bf7ef44015a3a624856bf67874003c0fa94b772774c` |
| `pkg/server/grpc.rs` | `e1ffda8db8156c2b6eef03f617be2adb8ff9a1c739926b7f812aa0aca9b8fc16` |
| `pkg/server/lib.rs` | `854a5ed327096f950b5eaac64599e6f286ae566558c651cd625d347f8603a8bc` |
| `pkg/utils/keypath/absolute_key_path.go` | `84b851ba31ba4f974622c5e4b2331387eb11108b3299fe88a5df67ace26a88d3` |
| `pkg/utils/keypath/cluster_paths.rs` | `8375b83369e53f51b569e9996704445f10ec24980557604a3d4db17c10cafd75` |
| `pkg/utils/typeutil/conversion.go` | `9dbd47d2bcba379aeb1caaa1dd8eda02ed57dc46db71e17bb62c900fe681b71f` |
| `server/cluster/cluster.go` | `a6bc285063b31d56ce4db42bf26f9409144ea35d05a90a7f0dace98a8add291e` |
| `server/cluster/cluster_test.go` | `11c9f4767ab374ee4920d7a54e4411c031afbec9be61af191102e0b8cea4f71d` |
| `server/cluster/cluster_worker.go` | `3d43809d617072b5bb58f5bbae99436de457aa6e889a75c5b75e215a55523a22` |
| `server/grpc_service.go` | `16fe2a147fe3d34966ca08c59c429871bf56242879839b56ded50af34e8dc088` |
| `server/server.go` | `829eb592771c69f68f0c5efb8b6a88565828c3b0d5430d01c8ec60948fb2ad80` |
| `tests/cluster.go` | `7974ada125a863ece987bb8f6c65844429a7413daef3f1c8095567468cc6c523` |
| `tests/integrations/client/client_test.go` | `1a9843cb87c77fbfdd5af0d729706b347451792a1807de785467d3e4897e7f38` |
| `tests/integrations/mcs/tso/keyspace_group_manager_test.go` | `5179b122d81b90e951a206760631295e37243c104f8b69ac100c0a2813dde156` |
| `tests/server/cluster/cluster_test.go` | `df21a0e8db96c94ea64bdaf2719ef2685cdd0f952399e7124e7ea1094a879313` |
| `tests/server/id/id_test.go` | `acdd90c4685e08ce8ae887bf0cb326a1161a56955ee63f9939151a8e45bfb566` |
| `tests/testutil.go` | `663d6691781ec6e1d2fa52822fc1c49d7a87988c89519d4cb606407c1d5f58df` |
| `tools/pd-ctl/tests/keyspace/keyspace_group_test.go` | `d57ca1990bfc6eb045e80af9f5b7029466957107c0f59d90084e98fe180c57e0` |


## 任务 2：多个标识请求（批次 71）

日期：2026-10-23（Asia/Shanghai）。准入：批次 70 的提交 `6dde3875b`
及上文盘点已核验，批次 69 的产物/证据链完整。当前 Go `id.go`、
`id_test.go` 相对规划基准无差异。只扩展 canonical
[allocator.rs](../../pkg/id/allocator.rs) 的 `Allocator::alloc_count(u32)`；
`alloc()` 委托 count=1，既有调用方接口保留。
新增 [allocate_count_test.rs](../../pkg/id/allocate_count_test.rs)，
在 [lib.rs](../../pkg/id/lib.rs) 显式注册。无依赖、manifest、锁文件变化。

### Go 依据与兼容范围

只读 oracle 为 `pkg/id/id.go` 的 `Alloc`（113–133）：持锁逐次消费，
返回最后一个 ID 和请求数量，count=0 返回当前 base，失败保留已消费状态。
相邻 `TestMultipleAllocator` 与 `TestIDAllocationEndValue` 的断言已只读复核；
它们没有 count>1 覆盖，批量 Go 服务测试及偏一枚举限制见上文。
`pkg/utils/etcdutil/etcdutil.go` 调用链包含 failpoint 注入点，因此本轮未运行
`go test -mod=readonly ./pkg/id -count=1`，也未启停 failpoint；
上述 Go 期望明确为源码依据，不是新增 Go 运行输出或跨语言运行差分。
本任务不改变 key/value 编码或新增 codec，原有八字节大端适配原样复用，
测试对持久边界解码后比较数值，不将 Rust 字节输出冒充 Go 编码证据。

批量请求在同一把 window 锁内跨窗口逐个分配，成功 `Ok((last, count))`，
失败只返回已有 `AllocatorError`，不暴露成功批次或回滚先前消费。
count=0 即使取消、旧任期或耗尽仍只读取本地 base。
非零请求逐 ID 保留现有 Rust term validation；预留仍经同一 Leadership
事务 fencing，传输错误仍直接上返，不盲目重试不确定提交。
保留既有 saturating u64 上界保护；与 Go 溢出及缓存 term 校验的差异见批次 70。
SetBase、namespace 与服务层 count 装配仍留给各自任务。

### 同一断言的后端覆盖

`allocate_count_memory` 与 `allocate_count_etcd` 共用真实存储断言：

- 新建 count=0 返回 (0,0)，不创建 ID 边界；step=3 请求 5 返回 (5,5)，
  持久边界 6；随后 count=0 返回 (5,0)，单值接口返回 6。
- 16 个 worker 各 10 次请求 5（跨 step=3 窗口），由每个返回末尾值展开
  本实例连续区间，精确覆盖 7..=806，共 800 个不同 ID，无交错重叠。
- 边界 MAX-2、step=1 请求 3：前两次消费至 MAX 后返回 Exhausted；
  count=0 返回 (MAX,0)，持久边界 MAX，后续非零仍耗尽，无回绕。
- 本地 808/end=810 后将持久值写成非法字节，请求 3 先消费 809/810，
  下次预留返回 CorruptBoundary；count=0 证明本地仍为 810。
  恢复边界 810 后下一次返回 811，未重用失败前消费值。
- revoke 旧 leader 后非零批次返回 NotLeader，零请求仍为 (811,0)；
  新任期 allocator 从持久边界 813 后返回 (815,2)。取消后非零返回
  Cancelled，零请求仍为 (815,0)。未使用模拟核心事务。

etcd 入口复用 canonical KV fixture（随机 loopback 端口、独立数据目录）。
已审查其 Go fixture 只读取既有源文件，`go build` 输出二进制到当前槽位，
不注入 failpoint 或生成源码；所有验证继承 `GOFLAGS=-mod=readonly`。
workspace 使用既有 proto 构建缓存，本轮没有运行协议生成或新增 Go 文件。

### TDD 与验证记录

初始最小可编译接口逐次调用原 alloc，但 count=0 固定返回 0；
同一组测试在两个真实后端均因实际 (0,0) != 期望 (5,0) 失败，
不是缺符号/环境失败。修复后同一测试与断言全部保留通过。

本阶段动态按顺序领取槽位 **1**，锁 owner 记录本任务、会话与存活持有 PID。
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁由持有进程退出 trap 清理；结束时等待所有 Cargo/fixture 子进程后释放，
保留编译缓存。所有命令在仓库根执行，继承该绝对路径与上述 GOFLAGS。

| 命令 | 退出码与有效测试数 |
| --- | --- |
| `cargo fmt --all`（红灯接口及最终实现后各一次） | 0；审查无其他 Rust 文件格式差异。 |
| `cargo test --locked -p astersql-pd-id --lib allocate_count -- --nocapture`（红灯） | 101；0 passed / 2 failed，均为零请求返回 base 断言。 |
| 同上（绿灯） | 0；2 passed / 0 failed / 0 ignored。 |
| `cargo metadata --format-version 1 --no-deps` | 0；非行为检查。 |
| `cargo fmt --all -- --check` | 0；非行为检查。 |
| `cargo check --workspace --locked` | 0；非行为检查。 |
| `cargo test --workspace --locked` | 0；638 passed / 0 failed / 1 既有 ignored（含 doc tests；非零行为测试实际执行）。 |
| `git diff --check`、证据链接检查 | 0；相对链接均存在。 |
| Go/模块开始结束 SHA-256 与路径检查 | 0；1101 个文件完全一致，相关 Git 状态为空。 |

状态：已完成。实际槽位 1 已在所有验证及 fixture 子进程结束后释放，
保留槽位缓存；红灯失败遗留的本任务独立 etcd 数据目录已清理。
已审查全部提交差异，只包含 allocator、显式测试注册、独立测试及本节证据；
仓库原有其他计划的状态修改/删除均排除。编译仅有既有 deprecated 字段及
复用 fixture 未使用 command 方法的告警，不影响行为验证。

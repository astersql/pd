# 存储选主与租约覆盖证据

## 任务 1：盘点选主与租约差距（批次 59）

日期：2026-10-23（Asia/Shanghai）。状态：已完成。范围仅为源码与测试清单，
不代表后续批次的实现或运行验收。TDD 不适用，没有运行 Go、Cargo、生成器、
格式化、依赖整理或 failpoint 命令；下文行为均为源码事实或明确的源码推导。

### 准入及固定基准

当前 HEAD：`ded39bac484253197e516fc62f2d37a3034468c3`。
已核验批次 58 提交 `ded39bac4` 实际只增加
[存储门面证据](theme3-存储门面.md) 任务 5；该节记录已完成、storage 双后端
27 passed、workspace 610 passed / 0 failed / 1 既有 ignored、筛选 Go 用例
8 + 1 passed，及未运行 Go 故障用例的边界说明。与 Storage 的实现、独立测试、
模块注册和依赖实物相符，批次 59 准入满足。没有把前序任务文件删除当作完成证据，
没有把前序日志冒充本轮测试结果。

已读 [仓库指南](../../AGENTS.md)、[Copilot 指南](../../.github/copilot-instructions.md)、
[ADR-001](architecture.md)、[移植指南](porting-guide.md) 和本任务只读计划。
`rg --files -g AGENTS.md -g '!target/**' -g '!.git/**'` 仅返回根 AGENTS.md。
规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 至当前 HEAD：
两个 Go 生产来源无变化；member.rs 仅从 storage 重导出切到 canonical storage-kv
导入及 Result 路径，任期行为没有变化。

固定来源（Git blob，均已逐行阅读，包含多行签名和私有辅助函数）：

| 文件 | HEAD blob |
| --- | --- |
| [leadership.go](../../pkg/election/leadership.go) | `55ebbfe0cf4c0eb55e1d616623d1d2580e5cc18e` |
| [lease.go](../../pkg/election/lease.go) | `dba422c62e9357e294c566d588713daac39da894` |
| [member.rs](../../pkg/member/member.rs) | `7f5049c4a2a4bd0fe14666f5fc18da15dc906a54` |
| [leadership_test.go](../../pkg/election/leadership_test.go) | `c3daf9d4d24f259829409ccaaf5af8f9f1eeb699` |
| [lease_test.go](../../pkg/election/lease_test.go) | `2741084710e24b6c271d551a3f15c111a77f1950` |
| [member_test.rs](../../pkg/member/member_test.rs) | `f19e10a2245b21096b93b6fe422d8333422364b5` |
| [etcd_behavior_test.rs](../../pkg/member/etcd_behavior_test.rs) | `00e5c2b88dcf9241bd26e4458b633ebe96be5e8b` |

### Go 全部公开符号与归属

后续任务号指本子计划任务 2–8（批次 60–66），是实现归属，均未在本任务实现。

| 公开类型、函数及方法 | 实际语义与复用/差距归属 |
| --- | --- |
| `GetLeader` | 按给定 leaderPath 解码 pdpb.Member 并取 mod revision；缺失返回 nil/0/nil，错误返回 nil/0/error。任务 7 的观察快照边界；member 身份及协议转换归后续“成员契约补齐”，领域层不引入 protobuf。 |
| `Leadership`、`NewLeadership` | 可变管理器，保存指定 key、purpose/name、atomic lease/value、取消句柄及 campaignTimes。任务 2 抽取唯一任期；任务 3 支持指定 key；任务 5/8 管理续租/reset。 |
| `GetLease`、`SetLease` | nil 接收者安全；SetLease 与 reset 发布锁串行。任务 4/5/8；不原样暴露 etcd 对象到领域 API。 |
| `GetClient` | nil 安全地暴露 etcd client；Rust 改用宿主 Arc<dyn MetaStore>，任务 2/3 复用，不增加第二套客户端。 |
| `GetLeaderKey`、`GetLeaderValue` | nil/未设置返回空字符串。任务 2/3 的任期值及指定键边界。 |
| `LeadershipTerm`、`CaptureTerm` | 捕获 key/value/leaseID，三者均非空/零才返回 true；不检查 IsExpired。任务 2 保留不可变快照；任务 8 明确捕获与 reset 发布的并发边界。 |
| `LeadershipTerm.Comparisons`、`LeaderValue`、`LeaseID` | 比较 Value+Lease，值与 ID 为捕获常量。任务 2；Rust 已额外比较 ModRevision，不能在提取时丢失。 |
| `GetCampaignTimesNum`、`ResetCampaignTimes`、`AddCampaignTimes` | 懒清理五分钟窗口，只在 Add 时删除旧时间；Get 只返回当前长度。归“成员契约补齐”的候选资格/选举频率策略，不属于本次 term 核心。 |
| `Campaign` | 先发布新 lease/value，再 Grant；附加 cmps 与 CreateRevision(key)=0 原子 Put(value, lease)。事务错误/冲突 Close 新 lease，Grant 错误直接返回。任务 3；不能假定 Go 所有失败分支已清空管理器。 |
| `Keep`、`Check` | Keep 在发布锁下捕获 lease/安装取消句柄，启动 KeepAlive；Check 只看本地 lease 未过期，不读后端 key。任务 4/5，Rust validate 的远程检查不能充作本地保守过期。 |
| `LeaderTxn` | 原有 cmps 追加私有 leaderCmp（只比 Value）。任务 2 复用 Rust 更强的三重 fencing；调用方不允许绕过任期事务。 |
| `DeleteLeaderKeyByRevision` | ModRevision 比较后 Delete，失败分支 Get 同一 key；若已经不存在也成功并 Reset，否则冲突且不 Reset。任务 6。 |
| `Watch` | 给定 revision 观察指定 key；删除、取消、watch 错误或长期不健康退出；处理进度、重建及压缩。任务 7/8。 |
| `Reset` | 锁内 detach lease、取消并清空句柄、清空 value，锁外只 Close 捕获的旧 lease；不能清除/关闭其后发布的新任期。任务 5/8。 |
| `Lease`、`NewLease` | etcd lease 客户端、ID、配置超时、本地过期、原子 closed、指标；purpose/name 用于故障定位。任务 4/5；指标归后续运维主题。 |
| `GetID`、`GetTimeoutSeconds` | nil/未 grant 为 0；超时为配置值秒数，不是后端当前剩余 TTL。任务 4。 |
| `Grant` | 请求开始时刻 + 响应 TTL 形成保守过期；配置 timeout 用于后续调度，成功后允许再次 Close。任务 4。 |
| `Close` | nil/重复为成功；先 closed CAS、注销指标、expireTime=ZeroTime，再撤租，最后关闭 lease client。撤租错误只记日志，最终返回 lease.Close 错误。任务 5/8；Rust resign 的错误传播目前不同，不可声称逐错误等价。 |
| `IsExpired` | now.After(expire)，相等时不算过期；nil/未初始化取 ZeroTime。任务 4 的可控单调时间/边界验收。 |
| `KeepAlive` | 配置 timeout/3 发请求，timeout 无成功响应则退出；只应用递增 maxExpire；写过期前再查取消。任务 4/5。 |

私有路径同样覆盖：leadership 的 `leaderCmp`、`removeLeaderKey` 分别归任务 2、6；
lease 的 `matchesFailpointTarget`（purpose@name 匹配）、`setID`、`loadExpireTime`、
`keepAliveWorker` 分别支撑任务 5、4、4、5。worker 每轮立即发请求，然后等 ticker；
子请求可重叠，单次请求 timeout 为配置租期；expire 从请求调度前的 start + 响应 TTL
计算。TTL<=0 不投递；错误不延长本地过期；向通道投递使用父 ctx 取消而非单请求 ctx。
这些是源码规则，未以 sleep 或新 mock/Go 工具验证。

### Rust 全部公开符号及复用边界

[member crate](../../pkg/member/Cargo.toml) 生产只依赖 storage-kv、thiserror、tokio、
tokio-util；[lib.rs](../../pkg/member/lib.rs) 重导出 member，显式注册 member_test，
Cargo 显式注册 etcd_behavior target。当前 election 目录只有 Go，尚无 election Rust crate。

| 公开符号 | 当前行为 / 归属 |
| --- | --- |
| `MemberError`、`Result` | Store、Cancelled、IdentityConflict、Contended、NotLeader、InvalidArgument；任务 2 统一基础错误归属并转换/重导出，避免 election→member。 |
| `Member`、`bootstrap` | 校验非零 cluster/member 和非空 value，CreateRevision=0 写身份；冲突时线性读匹配精确 key/value，否则 IdentityConflict。身份保留于 member，不随 term 迁移。 |
| `Member::campaign` | grant 后对固定 /pd/{cluster_id}/leader create-if-absent；成功返回新不可变 Leadership，失败尝试 revoke 本 lease；revoke 失败暂靠 TTL，未保留错误。任务 2 复用唯一实现，任务 3 增加指定键/额外比较。 |
| `Leadership`、`lease_id`、`revision`、`cluster_id`、`uses_store` | 固定 store/cancel/cluster/key/value/lease/revision；通过 Arc::ptr_eq 校验后端一致。任务 2 移动后保留 member 兼容出口和调用方语义。 |
| `validate` | 空写事务比较 value/lease/mod_revision；失败 NotLeader；取消优先。任务 2 保留，非本地 TTL 检查。 |
| `transact` | 禁止写/删自身 leader key；追加三重比较；失败时 validate 区分 NotLeader 与调用方 compare 失败（返回 succeeded=false）。任务 2；任务 6 的按 revision 清理必须是专用管理接口。 |
| `renew` | validate→keep_alive→validate；LeaseNotFound→NotLeader，未保存响应 TTL 或本地过期。任务 4/5 补足保守期限及受控循环。 |
| `resign` | 即使 token 已取消也能 revoke 本 lease，LeaseNotFound 幂等成功；没有本地 active/value detach 管理器。任务 5/8，保持旧任期不能影响新任期。 |

私有 `cancellable` 用 tokio::select biased 先查取消；`comparisons` 生成三重 fencing。
不新建第二套 term/MetaStore，不把租约有效等同于 key 仍属于该任期。

### Key/value、事务、观察与装配依赖

- Go key 来自 NewLeadership 参数，可用于 PD leader 或 Participant 的 election path；
  value 是调用方字符串（MemberValue 为协议字节），核心层无需解码，只有 GetLeader
  和 skipGrantLeader 分支依赖 pdpb.Member。Rust bootstrap 另写
  `/pd/{cluster_id}/rust/members/{member_id}`（无 lease），选主固定
  `/pd/{cluster_id}/leader`（绑定 lease），任期值直接复用传入 Vec<u8>。
  `/rust/members` 是现有 Rust 身份键，不宣称为 Go etcd 成员管理键的替代。
- campaign create-if-absent、term fencing、按 mod_revision 删除是不同事务边界。
  Go CaptureTerm 并非持有发布锁的一次原子多字段读取（先 lease 后 value）；Rust
  不可变字段不需要从可变管理器拼接。该差异是源码推导，任务 2/8 需固定安全契约，
  不能把可能混合的 Go 并发快照当作必须复制的行为。
- 复用 [MetaStore](../../pkg/storage/kv/meta_store.rs)：read 返回 revision，
  transact/branches/raw 支持原子比较和条件分支；watch 为 prefix、inclusive start，
  MetaWatch::next 拉取、取消终止、Drop 释放；Unavailable 可在 next 重试，
  Compacted 明确要求重新 snapshot+watch。lease API grant/keep_alive 返回 id/ttl_seconds，
  revoke/ttl/compact 已有；MemoryStore/EtcdMetaStore 都在 canonical kv crate。
- Go Watch 是 exact key；Rust prefix watch 必须过滤相邻 key。Go 非删除事件后
  revision=header.revision+1，压缩时 revision=CompactRevision 后重建；progress 不推进
  revision。Rust 必须按自己的 Compacted 契约显式重建快照，不能只机械抄 Go revision。
  任务 7 覆盖删除/取消/断连/无重复事件；任务 8 覆盖快照间隙/压缩恢复。
- [Go member](../../pkg/member/member.go) 和 [Participant](../../pkg/member/participant.go)
  持有 election.Leadership；调用 Campaign/Check/Watch/Reset，处理观察到自己的旧 key
  时调用 DeleteLeaderKeyByRevision；member 还通过 campaignTimes 控制频率和
  LeaderTxn 更新成员优先级。[election 接口](../../pkg/member/election.go) 为 TSO 等复用。
  [加密管理器](../../pkg/encryption/key_manager.go) 使用 Leadership/LeaderTxn，
  [TSO endpoint](../../pkg/storage/endpoint/tso.go) 使用领导身份，
  [keyspace](../../pkg/keyspace/tso_keyspace_group.go) 和
  [微服务清理](../../server/microservice_cleanup.go) CaptureTerm 后使用 Comparisons。
  [TSO keyspace group manager](../../pkg/tso/keyspace_group_manager.go) 还安装 Leadership
  campaign checker。全仓 `election.GetLeader` 检索未命中调用，member/Participant
  直接调用 etcdutil.GetProtoMsgWithModRev；不能凭该函数存在推断已被装配。
  这些消费者只读盘点，业务迁移归对应上层主题，不在本任务改装。
- Rust [ID](../../pkg/id/allocator.rs)、[TSO](../../pkg/tso/oracle.rs) 持有
  Arc<Leadership> 并用 uses_store、cluster_id、validate/transact；
  [runtime](../../pkg/runtime/node.rs) bootstrap/campaign 后以一秒 ticker 调 renew，
  取消/错误退出后 join 服务器并 resign。该宿主循环存在，不等于任务 5 的独立可控续租
  已实现；提取需保留这些公开消费者，后续启动装配归 runtime 主题。
  [根 facade](../../pkg/lib.rs) 当前只重导出 runtime API，根 Cargo 声明 member；目标依赖为
  member→election→kv，election 不能引用 member/runtime/proto。

### 现有断言、故障点与未覆盖归属

| 只读测试依据 | 已有断言/场景（源码，非本轮运行） | 后续归属 |
| --- | --- | --- |
| Go TestLeadership | 两候选唯一成功；自然到期；Keep 后有效；Reset 后无 lease，晚到 Keep 不重启 | 任务 3/4/5 |
| Go TestResetDoesNotClearNewLeadership | afterRevokeLease 屏障，旧 Reset 阻塞时第二次 Reset 及同值/异值重选，新任期仍可 LeaderTxn 写入 | 任务 8 |
| Go TestKeepDoesNotRestartLeaseDuringReset | beforeRevokeLease 屏障，detach 后 Keep 不安装取消句柄，新 campaign 后可 Keep | 任务 5/8 |
| Go TestDeleteLeaderKeyByRevisionDoesNotDeleteChangedLeader | 旧 revision 失败且新 value 保留；正确 revision 删除；已缺失再次成功 | 任务 6，另补同值新 lease、同 lease 新 revision |
| Go TestExitWatch / checkExitWatch | 客户端启动前/运行中关闭、删除、服务端关闭、leader transfer、失去 quorum，观察应退出 | 任务 7/8；etcd 集群成员管理本身在上层主题 |
| Go TestRequestProgress | 正常 progress 日志、丢弃响应后 blocked 日志 | 任务 7/8；不能用日志或调用次数取代 Rust 实际终态验收 |
| Go TestCampaignTimes | 窗口懒清理与数量 | 成员契约补齐的频率策略 |
| Go TestLoadExpireTime / TestLease / TestLeaseKeepAlive | nil/空/非法值为 ZeroTime；grant、到期、close、regrant、续租、worker 响应 | 任务 4/5 |
| Rust behavior；memory_bootstrap_and_lease_loss / real_etcd_behavior | 身份幂等/冲突、单赢家、key lease/revision、renew、显式 revoke、旧 term 禁写、替代 term、取消后 campaign 失败 | 任务 2 保留原 member/id 回归，任务 3 补指定键及附加条件 |
| Rust expiry；memory_natural_expiry / real_etcd_natural_expiry | 自然到期后 validate/renew NotLeader，重选 revision 增长，旧 term 未写入 | 任务 4 保留远程到期，补本地保守时钟/边界 |
| Rust real_etcd_identity_recovery | fixture stop/start，同 WAL 身份保留且冲突，重新 campaign/resign | 已有身份恢复；不得据此声称所有 lease/reset 故障已覆盖 |

全部 Go 故障点：`skipGrantLeader` 在 grant 前按 leaderData 中 Member.Name 选择失败；
`fastTick` 将 Watch 不健康退出阈值从 60 秒改为 5 秒；`delayWatcher` 在健康检测前暂停；
`watchChanBlock` 收到响应后丢弃并回到接收循环（不更新时间）；
`beforeRevokeLease` 在本地过期清零后、撤租前；`blockLeaseClose` 按 purpose@name
在该处阻塞 10 秒；`afterRevokeLease` 在撤租后/lease client Close 前；
`keepAliveFailed` 在真实 KeepAliveOnce 请求后按 purpose@name 替换成错误，因此远端可能
仍活着而本地到期。分别归任务 3、7/8、7/8、7/8、5/8、5/8、8、4/5。
关联 etcdutil/fastTick 只作为既有 Go TestExitWatch 的辅助注入依据。

Rust 当前缺少：指定 key/额外比较（3）、本地 conservative expiry/TTL 取整与时钟漂移（4）、
续租超时/取消与 join/迟到响应（5）、按观测 revision 删除（6）、领导键观察（7）、
snapshot-watch 间隙与 Compacted、reset/campaign 交错（8）。没有未归属的后续项。
这些故障没有本轮 Go 运行输出，不构成盘点阻塞，但后续实现不得把本清单当作运行差分。
编码不在本任务更改；必要 wire 输出后续只能复用已有 fixtures/已保存输出或只读工具，
缺失时按该任务记录阻塞。不得为了补证据新增 Go oracle。

指标单列为后续运维主题：[metrics.go](../../pkg/election/metrics.go) 的续租响应间隔、
请求耗时、请求 tick 间隔、终止原因计数及本地 TTL collector，以及
[metrics_test.go](../../pkg/election/metrics_test.go) 的 TestLocalTTLRemainingCollector；
不在本子计划迁移全局注册器。集群成员管理、加密、完整 MCS、生产本地持久化与
启动装配均仍属上层主题，不据本盘点关闭来源史诗。

### 本轮验证

确切符号命令：

```text
rg -n '^(func |type |pub |    pub )' pkg/election/leadership.go pkg/election/lease.go pkg/member/member.rs
```

退出 0，命中 51 个声明行（含私有辅助函数）；逐行补读多行 bootstrap/transact 签名、
结构字段、Campaign/KeepAlive/Watch/Reset 全体控制流及相邻测试，不以命中数替代穷举。
Git blob 固定、反向依赖检索、规划基准差异审查均完成；相对 Markdown 引用逐个核验。
后续拟建 `pkg/election/{Cargo.toml,lib.rs,leadership.rs,leadership_test.rs,
lease_expiry.rs,lease_expiry_test.rs,lease_keep.rs,lease_keep_test.rs,
leader_delete.rs,leader_delete_test.rs,leader_watch.rs,leader_watch_test.rs,
watch_reset.rs,watch_reset_test.rs}` 仅为编号任务产物，不是已存在的源码。

`git diff --check`、`git diff --cached --check` 均退出 0；本任务仅新增本证据文档，
没有代码/测试/配置/模块变化。开始与结束 1101 个 Go/模块文件路径和 SHA-256 完全一致，
相关 Git 状态为空。未修改只读 plan.md 或来源主题；保留工作区既有的其他任务状态修改
及前序任务删除，并从本次提交排除。

## 任务 2：提取唯一领导任期（批次 60）

日期：2026-10-23（Asia/Shanghai）。前置批次 59 由提交 `197ab0e4d` 和上节
源码/依赖/测试清单核验；Go leadership/lease 自规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无变化。本任务为归属重构，
按任务约定不编造失败行为作为 TDD 红灯。

### 唯一实现、API 与行为

- [election crate](../../pkg/election/Cargo.toml) 生产依赖仅为 canonical
  storage-kv、thiserror、tokio、tokio-util，无 member、storage facade 或协议依赖，
  没有额外 feature。[leadership.rs](../../pkg/election/leadership.rs) 为唯一
  `Leadership` 定义，保留原 store/cancel/cluster_id/key/value/lease/revision 字段。
- 原 `Member::campaign(ttl_seconds)` 仍返回同一 `Leadership`；其事务实现移到
  `Leadership::campaign(store, cancel, cluster_id, value, ttl_seconds)`，路径仍为
  `/pd/{cluster_id}/leader`，grant、CreateRevision=0、冲突/错误撤销自身 lease 的
  顺序不变。Member 身份 bootstrap 仍留在 [member.rs](../../pkg/member/member.rs)。
- `lease_id/revision/cluster_id/uses_store/validate/transact/renew/resign` 签名和
  行为不变；将错误名归一后，原 `lease_id` 起的全部方法与旧实现逐字一致。
  [member 导出](../../pkg/member/lib.rs) 保留 `Leadership`、`MemberError`、`Result`；
  基础错误唯一归属 election 的 `ElectionError`，member 将其别名导出为
  `MemberError`，保留全部 variant、StoreError 转换和错误文本。保留 IdentityConflict
  variant 以维持旧 member API，election 不依赖身份注册实现。
- [原 member 场景](../../pkg/member/member_test.rs) 增加 canonical 类型赋值，
  编译确认 member 返回值与 election 类型相同。原 member、ID、TSO 场景均保留，
  没有需要移动的私有字段测试。Cargo.lock 仅增加本地 election 包，并将 member
  的 thiserror 直接依赖换为 election；没有新增外部依赖版本。

[独立 election 测试](../../pkg/election/leadership_test.rs) 显式注册到 lib，
`leadership_move_memory` 与 `leadership_move_etcd` 调用同一真实行为断言：
单赢家、key/value/lease/mod_revision 快照、store 身份、续租、成功的受保护写入、
调用方 compare 冲突、自身 leader key 写/删拒绝、同值同 lease 的 revision 更新后
旧任期拒写、同值新 lease 重选、旧任期幂等 resign 不伤新任期、换 value 后失效、
取消优先，以及取消后仍能 resign 并最终清空 leader key；拒绝写的应用 key 保持缺失。
真实 etcd 使用既有共享 fixture，随机 loopback 端口、独立目录和真实事务，不 mock
核心事务结果。自然到期行为继续由原 member memory/etcd expiry 场景覆盖：
每 50ms 验证、10s 截止，失效后 renew 返回 NotLeader，新任期 revision 增长且旧写
未落盘；本任务不改动到期机制，也不引入断连逻辑。

### Go 只读证据与编码边界

仅复用上节固定 blob 的 Go `LeadershipTerm/Comparisons/CaptureTerm`、
`Campaign/LeaderTxn`、`Reset` 和相邻测试断言作为源码依据：Go term 比较 value+lease，
既有 Rust 另有 mod_revision，提取完整保留。没有运行需 failpoint 的 Go election
测试，没有获得本轮 Go 故障运行差分，也没有将源码推导称为运行结果。
本任务无 codec 或协议转换，value 仍是调用方提供的原始 Vec<u8>；双后端读取断言
确认 opaque value 原样存储，不声称该测试数据是 pdpb.Member 编码。任务不新增编码，
不手抄 Rust 输出作为 Go 字节预期；既有协议编码证据仍见 proto fixtures，协议层不迁入
选主核心。现有 Rust fixture 的调用链已读：`go build -o <slot binary>
./scripts/rust-etcd-fixture` 只编译已有 Go 文件，运行后只写槽位内 etcd 数据，
无生成器/failpoint；后续命令额外继承 `GOFLAGS=-mod=readonly`。

### 本轮验证与完成证据

状态：已完成。每个 Cargo 阶段均从 1–10 依次原子 mkdir 动态领取，实际都领取
槽位 **1**；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划、任务、会话及持有 Python 进程 PID；包装进程在正常/失败退出清理自己的
owner 与锁，信号处理等待该阶段子进程退出后释放。全部阶段已结束、自己的锁已释放，
保留缓存。没有通过 target-dir 绕过槽位或并行修改 workspace。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-storage -p astersql-pd-member -p astersql-pd-id -p astersql-pd-tso -- --nocapture`（迁移前） | 0 | 53 passed / 0 failed / 0 ignored；storage 27、member 7、id 7、tso 12 |
| `cargo test --locked -p astersql-pd-storage -p astersql-pd-member -p astersql-pd-id -p astersql-pd-tso -- --nocapture`（迁移后） | 0 | 同样 53 passed，测试名称及重复计数逐项对应 |
| `cargo test --locked -p astersql-pd-election -- --nocapture` | 0 | 2 passed：leadership_move_memory、leadership_move_etcd |
| `cargo fmt --all`（Rust 修改后） | 0 | 仅格式化本任务 Rust 新增/修改文件 |
| `cargo check --workspace`（新增 crate 同步 lock） | 0 | lock diff 仅本任务两处依赖归属 |
| `cargo metadata --format-version 1 --no-deps` | 0 | election 注册、依赖方向及 member 导出审查通过 |
| `cargo fmt --all -- --check` | 0 | 格式检查通过 |
| `cargo check --workspace --locked` | 0 | workspace 编译通过 |
| `cargo test --workspace --locked` | 0 | 612 passed / 0 failed / 1 既有 ignored；新增两个后端入口均通过 |
| `git diff --check`、`git diff --cached --check` | 0 | 已审查实现、测试、manifest、lock 和本节证据差异 |

零测试 doc targets 未计入有效测试数，既有 ignored 不作为本任务验收场景。
开始/结束核对 1101 个 Go/模块文件路径与 SHA-256 完全一致，Go/模块 Git 状态为空。
只读 plan.md 和来源史诗无差异；工作区既有其他任务状态修改/删除均保留并排除提交。
此证据仅证明任期提取与兼容导出，不代表后续 lease/watch/reset 任务完成。

## 任务 3：支持指定键选主（批次 61）

日期：2026-10-23（Asia/Shanghai）。前置批次 60 的提交 `9a68e3b32`、上节
完成证据、canonical election crate 及 member 兼容导出已核验，准入满足。
Go `pkg/election/leadership.go` 自规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异，仍为上节固定 blob。
本任务原位扩展 [leadership.rs](../../pkg/election/leadership.rs)，不建立第二套 term。

### 行为、Go 依据与边界

- 新增 `Leadership::campaign_with_key(store, cancel, cluster_id, key, value,
  ttl_seconds, compares)`；原 `Leadership::campaign` 委托它，继续使用
  `/pd/{cluster_id}/leader`。member 原调用路径与 API 不变。
- Go `NewLeadership` 保存调用方 leaderKey；`Campaign` 先 grant，再将调用方
  cmps 与 `CreateRevision(leaderKey)=0` 组合，原子写 opaque value 并绑定 lease。
  事务错误/比较冲突关闭本次 lease。Rust 保留同一顺序与既有错误分类：比较失败
  `Contended`，后端错误 `Store`；所有事务失败分支继续尝试撤销自己的 lease。
  后端不可用导致 revoke 失败仍依赖 TTL，这与原路径一致，本任务没有扩大故障恢复范围。
- Go `TestLeadership` 的同键两候选断言及 `Campaign` 事务是只读源码依据，
  不冒充本轮 Go 运行差分。整包 Go 命令包含依赖 failpoint 的 election 测试，
  因 Go 只读约束没有运行 `go test -mod=readonly ./pkg/election ./pkg/member
  ./pkg/id -count=1`，没有启停 failpoint。此任务无 codec/协议转换：二进制 value
  只作 opaque 字节原样存取，不声称其为 Go Member 编码，不需要新增 Go 编码输出。
- [campaign_key_test.rs](../../pkg/election/campaign_key_test.rs) 在 lib 显式注册，
  `campaign_key_memory` / `campaign_key_etcd` 共用全部断言：同一 store、同一 cluster
  的相邻键 `/election/a`、`/election/ab` 独立成功，value/lease/revision 精确保存；
  自身 leader key 禁写；同键冲突不影响既有任期；附加条件失败不占键且真实 lease
  已不存在，条件改变后可成功；无效比较的后端错误也撤销本次 lease；两个并发候选
  恰有一个成功；默认路径可同时选主；退出一个任期不影响其他键，最终所有已 grant
  lease 的 TTL 查询都返回 LeaseNotFound。
- RecordingStore 仅记录真实 grant 的 ID，所有事务、读、watch 和 lease 操作原样
  委托真实 MemoryStore/EtcdMetaStore，不模拟核心事务结果。测试专用 async-trait
  复用 workspace 已锁版本；Cargo.lock 只增加 election 的这一个既有依赖名。
  生产依赖、根 manifest 无变化。共享 etcd fixture 沿用随机 loopback 端口、独立数据
  目录，调用链已读，仅 `go build` 现有只读源码及运行 etcd，继承
  `GOFLAGS=-mod=readonly`，不生成/改写 Go。本任务没有断连或到期场景。

### TDD 与验证记录

初始可编译的最小入口保留固定 key、忽略附加比较，未预先实现目标行为；
第一次聚焦验证退出 101：两个后端均在第二个不同 key 选举的 unwrap 处收到
Contended（0 passed / 2 failed）。仅接通 key 后第二次退出 101：两个后端均在
附加条件失败应返回 Contended 的真实断言失败（0 passed / 2 failed）。
随后将调用方 compares 追加 CreateRevision=0，原测试不改断言、不忽略场景，获得绿灯。

状态：已完成。每阶段均依次原子 mkdir 动态领取共享槽位，实际均为 **1**；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、任务、会话及持有 Python 进程 PID；包装器正常/失败退出均释放自己的锁，
信号处理等待子进程组退出后释放。全部 Cargo 与 fixture 进程已结束，锁已释放，保留缓存。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all`（每次 Rust 修改后） | 0 | 只涉及本任务 Rust 文件 |
| `cargo check --workspace`（同步测试依赖 lock） | 0 | lock 仅增加已锁 async-trait 依赖关联 |
| `cargo test --locked -p astersql-pd-election --lib campaign_key -- --nocapture`（固定 key 红灯） | 101 | 0 passed / 2 failed，第二个不同键选举错误冲突 |
| `cargo test --locked -p astersql-pd-election --lib campaign_key -- --nocapture`（附加比较红灯） | 101 | 0 passed / 2 failed，条件不满足却选举成功 |
| `cargo test --locked -p astersql-pd-election --lib campaign_key -- --nocapture`（最终绿灯） | 0 | 2 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 默认 feature，生产依赖方向不变 |
| `cargo fmt --all -- --check` | 0 | workspace 格式检查通过 |
| `cargo check --workspace --locked` | 0 | workspace 编译通过 |
| `cargo test --workspace --locked` | 0 | 614 passed / 0 failed / 1 既有 ignored，含 member/ID/TSO 原回归及本任务两个后端入口 |
| `git diff --check`、`git diff --cached --check` | 0 | 实现/测试/依赖/证据差异已审查 |

零测试 target 不计入有效场景，既有 ignored 不作为本任务验收证据。开始/结束的
1101 个 Go/模块文件路径和 SHA-256 完全一致，相关 Git 状态为空。
只读 plan.md、来源主题无本任务差异；既有其他任务状态修改/删除均保留并排除提交。
本节仅覆盖指定键及附加比较，不表示 lease/watch/reset 后续批次完成。

## 任务 4：实现租约保守过期判断（批次 62）

日期：2026-10-23（Asia/Shanghai）。已核验前置批次 61 提交 `3885a20c1`、
上节完成证据及指定键选主实物，准入满足。Go `lease.go`、`lease_test.go`
自规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

### 行为与只读依据

- Go `Lease.Grant` 从请求前 `time.Now()` 加响应 TTL 计算期限；`IsExpired`
  使用严格 `After`；`loadExpireTime` 对 nil/未初始化取 ZeroTime。
  `keepAliveWorker` 在请求调度前计时，非正 TTL/错误不更新；`KeepAlive`
  只应用递增期限；`Close` 在远程 revoke 前清空本地期限。
  `TestLoadExpireTime`、`TestLease`、`TestLeaseKeepAlive` 的源码断言作为只读依据。
  计划整包 Go 命令含需 failpoint 的相邻测试，因此未运行
  `go test -mod=readonly ./pkg/election ./pkg/member ./pkg/id -count=1`，
  未启停 failpoint；这里没有本轮 Go 运行差分，也没有将源码推导冒充运行结果。
- [lease_expiry.rs](../../pkg/election/lease_expiry.rs) 实现单调时钟期限；
  [leadership.rs](../../pkg/election/leadership.rs) 在现有 canonical 任期的 grant、
  显式 renew、resign 路径接入。`Leadership::is_expired` 是无远程 IO 的本地判断，
  `validate` 继续承担后端 ownership fencing，二者语义分别对应 Go Check 和事务比较。
  `campaign_with_key_and_clock` 允许宿主注入单调时钟，原有入口默认使用 `Instant`。
  不添加私有 runtime、续租循环、配置、依赖或平行任期实现。
- 期限保留纳秒精度，按后端响应的整数 TTL 秒计算，不使用请求 TTL 或剩余 TTL
  的向下取整值；相等仍有效，晚 1ns 过期。续租取既有期限与新期限的最大值；
  非正/溢出 TTL 不延长。未 grant/close 为过期，close 后晚到响应不能复活，
  fresh grant 可以初始化新期限。锁内更新与清空串行，不在锁内进行存储 IO。
- 生产计时接口仅返回 `std::time::Instant`，完全不读取 `SystemTime`/墙钟，
  系统墙钟校正无法改变期限；没有修改宿主系统时钟。测试注入单调时间推进，
  不以长 sleep 证明到期。此项为时钟类型/调用路径审查与单调边界测试的证据，
  不宣称运行了操作系统改时实验。

### 测试时序与 TDD

[lease_expiry_test.rs](../../pkg/election/lease_expiry_test.rs) 在 lib 显式注册，
`lease_expiry_boundaries` 验证未初始化、grant、纳秒/相等边界、乱序续租、非法 TTL、
close 后晚到响应及 fresh grant。`lease_expiry_memory` / `lease_expiry_etcd`
复用相同后端断言；IO 包装器仅延迟/丢弃响应，全部事务委托真实 MemoryStore/etcd。

双后端时序：grant 请求起点 → 后端真实 grant → 注入 2s 响应延迟 → 到达
请求起点 + 响应 TTL 时有效 → 再推进 1ns 即过期；此时后端 lease/key 仍存在，
远程 validate 成功。显式 renew 同样扣除 2s 延迟，随后响应失败不延长期限；
成功响应若延迟 100s，返回时本地已经过期。另以请求 TTL=1、真实响应 TTL
验证后端取整与严格相等边界，再测试整个 grant 响应晚于期限。最终撤租后
TTL 查询为 LeaseNotFound，本地保持过期。

etcd 附加真实故障：grant 30s → fixture stop 并确认 stopped → renew 失败 →
单调时钟推进 31s → 本地过期；断连时 resign 返回错误但本地仍过期 →
同 WAL restart 并确认 restarted → resign 成功清理。fixture 随机 loopback 端口与
独立数据目录，沿用已有只读 Go fixture；仅 `go build` 已有源码，继承
`GOFLAGS=-mod=readonly`，不运行生成器或 failpoint。任务无 codec，无新增字节编码。

可编译最小接口最初将期限设为响应时刻；行为红灯实际执行 3 个测试，退出 101，
0 passed / 3 failed：纯值在 fractional seconds 断言失败，两个后端在应有效的
TTL 边界断言失败。此前一次测试编译错误（Unavailable 是 unit variant）已修复，
不计作行为红灯。实现请求起点 + TTL 与最大期限后，同一组原始断言全部转绿；
再补充真实断连、TTL 取整及慢 grant 场景，未削弱/忽略原断言。

### 本轮验证

每阶段依次原子 mkdir 动态领取共享槽位，实际均为 **1**；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁 owner 记录计划、任务、会话与持有 Python PID；正常/失败及中断处理均等待
该阶段子进程组退出后释放自己的记录和锁，保留缓存。

状态：已完成。全部阶段 Cargo、fixture 子进程已退出，自己的锁已释放。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all`（Rust 修改后） | 0 | 仅本任务 Rust 文件被格式化 |
| `cargo test --locked -p astersql-pd-election --lib lease_expiry -- --nocapture`（行为红灯） | 101 | 0 passed / 3 failed，真实期限断言失败 |
| `cargo test --locked -p astersql-pd-election --lib lease_expiry -- --nocapture`（最终绿灯） | 0 | 3 passed / 0 failed / 0 ignored，纯值及两个真实后端入口 |
| `cargo metadata --format-version 1 --no-deps` | 0 | crate/依赖/默认 feature 不变 |
| `cargo fmt --all -- --check` | 0 | workspace 格式通过 |
| `cargo check --workspace --locked` | 0 | workspace 检查通过 |
| `cargo test --workspace --locked` | 0 | 617 passed / 0 failed / 1 既有 ignored |
| `git diff --check`、`git diff --cached --check` | 0 | 实现、测试、模块注册及本节证据已审查 |

零测试 target 不计入有效测试数，既有 ignored 不作为本任务证据。开始/结束
1101 个 Go/模块文件路径及 SHA-256 完全相同，相关 Git 状态为空。
根 Cargo.toml、Cargo.lock、crate manifest、只读 plan.md 与来源主题均无本任务差异。
既有其他任务状态变更/删除保留并排除提交。本节不代表后续自动续租、watch、reset
或启动装配任务已经完成。

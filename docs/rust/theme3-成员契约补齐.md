# 成员持久化契约盘点

## 任务 1：盘点成员持久化缺口（批次 67）

日期：2026-10-23（Asia/Shanghai）。状态：已完成。本轮仅生成事实清单，未改变运行行为。
规划基准：`5d5948052a9ee9e3fa7a09145fe2916a2912c223`；审查 HEAD：
`ccbadfe93ee62404f3136217406d03ee3f10c943`。本文中的文件、行号和公开接口均固定于此 HEAD。

已阅读 [仓库规则](../../AGENTS.md)、[Copilot 指南](../../.github/copilot-instructions.md)、
[ADR-001](architecture.md)、[移植指南](porting-guide.md)、[依赖基线](dependencies.md)、
本子计划只读总计划及来源主题 3；仓库内只有根 AGENTS.md。
依赖批次 66 已准入：提交 `ccbadfe93` 包含
[watch_reset.rs](../../pkg/election/watch_reset.rs)、
[watch_reset_test.rs](../../pkg/election/watch_reset_test.rs) 和显式模块注册；
[存储选主证据](theme3-存储选主.md) 任务 8 记录聚焦测试 4 passed / 0 failed、
workspace 629 passed / 0 failed / 1 既有 ignored，以及格式、metadata、check 全部退出 0。
已对照提交及当前源码；这是复用的前序运行证据，不是本轮重跑。

### 来源文件与身份边界

逐行阅读以下四个目标文件；同时补读相关 keypath、election、runtime、测试和 manifest。
三个 Go 来源相对规划基准无差异；member.rs 的差异是 MetaStore 归属切至 storage-kv，
Leadership/错误类型抽至 canonical election crate，campaign 改为委托；bootstrap 身份规则保留。

| 来源 | SHA-256（工作树内容） |
| --- | --- |
| [pkg/member/member.go](../../pkg/member/member.go) | `2e637d2a57d70bdc37db846fff5c3ca824af1741000a6b1925b56c6351f8c102` |
| [pkg/member/election.go](../../pkg/member/election.go) | `6ef9e0ceb5bdaef53809e7aad5d65c88f62f7b08453d1b6dc3da1e4c2eb32731` |
| [pkg/member/participant.go](../../pkg/member/participant.go) | `5cc0d4118c3fde9c7f266749ff2c16a0e0a0c7d44c9b278401a2562ed4f4a776` |
| [pkg/member/member.rs](../../pkg/member/member.rs) | `f79bcc32e81a9e405deaea7dba67b7006260c4850d7baff3f1e098a4db10cc99` |

| 对象 | 当前事实及边界 |
| --- | --- |
| Go embedded etcd 成员身份 | `Member.id` 是 etcd server ID；`NewMember` 接收 embed.Etcd/client/id，不负责在此文件执行 join。`Etcd`、`GetEtcdLeader`、MoveLeader/ListEtcdMembers 操作具体 etcd 集群，不等于 PD leader lease。 |
| Go PD 成员信息 | `InitMemberInfo` 在内存构造 pdpb.Member：Name、MemberId、按逗号 Split 的 ClientUrls/PeerUrls；Marshal 成 memberValue 并创建 Leadership，没有把该消息写入持久成员目录。Go ID 恰好沿用 etcd ID，不可推导 Rust identity 能加入 etcd。 |
| PD 领导信息 | `/pd/{cluster_id}/leader` 保存 pdpb.Member wire bytes 并绑定 lease；内存 leader 缓存、IsServing 和 etcd cached Lead() 是不同状态。GetEtcdLeader 可能过期，不能单独证明可服务。 |
| PD 成员发布元信息 | 四个 `/pd/{cluster_id}/member/{member_id}/...` 字符串键见下表，与成员身份注册和 leader value 均不同。 |
| Rust identity | `/pd/{cluster_id}/rust/members/{member_id}` 为 Rust 新增不可变注册键；bootstrap 校验 cluster/member 非零和值非空，create_revision=0 原子注册；已有 exact key 同值幂等，异值 IdentityConflict。无 lease，选主租约丢失和真实 etcd 同 WAL 重启后可恢复。 |
| MCS Participant | ID/name/listen URLs 由服务 protobuf 提供，包含 MsParam 的 service/group；使用 `/ms/.../primary` 选主，不是 etcd 成员管理，也不是 PD 发布元信息。 |

保留 [member-id 基线](member-id.md) 的 `/rust/members` 语义，不把旧 key 自动搬入
`/member`，不把发布信息写入 identity，不通过覆盖 identity 更改 URL/name。
bootstrap 冲突判定会在线性 prefix 读取后再次过滤 exact key，不能把相邻 ID 误认成同一成员。
本轮未声称对所有非法输入或并发 bootstrap 都已有独立运行覆盖。

### 公开接口逐项归属

“已有”仅指明确列出的 Rust 原语；Go 的服务包装未迁移时仍标为“服务主题”，
不能因底层 lease/watch 存在而宣称整个 Member/Participant 已对齐。
“此计划 2”是批次 68 的六个发布信息读写接口；“此计划 3”是批次 69 的三个优先级接口。
服务主题归属固定为来源计划任务 7（PD 服务/API）、任务 8（MCS），
具体 etcd 后端生产成员管理和替换由任务 10 承担；本轮不执行这些主题。

#### member.go：40 个导出函数

| 符号（源行号） | 归属 | 事实、已有原语与未覆盖原因 |
| --- | --- | --- |
| `NewMember` :70 | 服务主题 7 | 持有 embedded etcd/client/id；Rust bootstrap 只注册 PD identity，不提供此构造契约。 |
| `Member.ID` :79 | 服务主题 7 | 返回 etcd server id；Rust bootstrap 接收领域 member_id，不承诺 etcd 身份。 |
| `Member.Name` :84 | 服务主题 7 | 读取内存 pdpb.Member.Name；Rust 只存 opaque value。 |
| `Member.GetMember` :89 | 服务主题 7 | 返回内存 pdpb.Member（any）；领域层不能照搬 protobuf。 |
| `Member.MemberValue` :94 | 服务主题 7 | 返回 Marshal 后 wire string；Rust 接受 adapter 编码 bytes，未提供此 getter。 |
| `Member.MemberString` :99 | 服务主题 7 | nil 返回空串，否则 protobuf String；没有领域格式化等价实现。 |
| `Member.Member` :107 | 服务主题 7 | 返回 *pdpb.Member；由服务适配层提供。 |
| `Member.Etcd` :112 | 服务主题 7 / 10 | 暴露 embed.Etcd；ADR 禁止通过 Go FFI 保留此对象。 |
| `Member.Client` :117 | 服务主题 7 | 暴露 clientv3.Client；Rust 经 MetaStore，不复制此具体类型 API。 |
| `Member.IsServing` :122 | 服务主题 7 | lease Check 且缓存 leader ID 等于本人成员 ID；Rust validate 仅覆盖任期 fencing。 |
| `Member.IsLeaderElected` :127 | 服务主题 7 | 检查 GetLeader 是否非 nil；Rust 无该缓存。 |
| `Member.GetServingUrls` :132 | 服务主题 7 | 读取缓存 leader 的 ClientUrls；Rust 无缓存/重定向包装。 |
| `Member.GetLeader` :137 | 服务主题 7 | atomic 缓存，nil 或 member_id=0 视为无 leader；Rust 无此 protobuf 缓存。 |
| `Member.PromoteSelf` :162 | 服务主题 7 | 发布自己到内存缓存并更新时间；不是持久化注册。 |
| `Member.GetElectionPath` :167 | 服务主题 7 | Go 使用全局 ClusterID；已有 ClusterPaths.election_path(None) 接收显式 cluster，但服务 getter 未迁移。 |
| `Member.GetLeadership` :172 | 服务主题 7 | 已有 canonical Leadership 的重导出；Member 不保存可变当前任期，未提供此 getter。 |
| `Member.GetLastLeaderUpdatedTime` :177 | 服务主题 7 | 返回缓存更新时间，未设置为零 time；Rust 无此状态。 |
| `Member.Campaign` :188 | 服务主题 7 | 已有 Member::campaign/create-if-absent；Go 频率计数超过阈值后转让 etcd leader、重置计数及专有错误未迁移。 |
| `Member.CheckLeader` :229 | 服务主题 7 | etcd leader 前置检查、读 proto+revision、自身残留键按 revision 删除、200ms 失败退避；底层 delete/observer 已有，服务组合未实现。 |
| `Member.WatchLeader` :269 | 服务主题 7 | 先发布 protobuf leader，阻塞观察后清空缓存；已有 LeaderWatch/LeaderObserver，缓存包装未实现。 |
| `Member.Resign` :286 | 服务主题 7 | 必须先清 leader 缓存再 Reset；已有 Leadership::resign/LeadershipSlot::reset，未覆盖 PD API/重定向缓存时序。 |
| `Member.CheckPriority` :292 | 服务主题 7 | 读取优先级、更新 gauge、比较并移动 etcd leader；此计划 3 只补存储，不移植循环/指标/转让。 |
| `Member.MoveEtcdLeader` :323 | 服务主题 7 / 10 | 5 秒 context timeout 后调用 embedded etcd Server.MoveLeader；属于具体后端和服务。 |
| `Member.GetEtcdLeader` :336 | 服务主题 7 / 10 | 返回可能陈旧的本地 Server.Lead()，含 staleEtcdLeaderView failpoint；Rust 无对应后端句柄。 |
| `Member.InitMemberInfo` :352 | 服务主题 7 | Split URL、构造/Marshal pdpb.Member、初始化领导对象；不是任务 2 的三个字符串持久化。 |
| `Member.ResignEtcdLeader` :373 | 服务主题 7 / 10 | from 仅记日志；列出成员、单节点短路、按名字/随机选候选并 MoveLeader；Rust 未实现成员发现/转让。 |
| `Member.SetMemberLeaderPriority` :400 | 此计划 3 | LeaderTxn Put 十进制 strconv.Itoa(priority)；内部事务错误和 compare conflict 分别返回。 |
| `Member.DeleteMemberLeaderPriority` :414 | 此计划 3 | LeaderTxn Delete；即便目标缺失也仍须通过 leader fencing。 |
| `Member.GetMemberLeaderPriority` :428 | 此计划 3 | 直接读，缺失 0；ParseInt(base=10, bitSize=32)，非法/越界报错，负数有效。 |
| `Member.GetMemberDeployPath` :445 | 此计划 2 | 直接读 deploy_path，缺失 ErrEtcdKVGetResponse("no value")。 |
| `Member.SetMemberDeployPath` :458 | 此计划 2 | os.Executable 后 filepath.Dir，单 Put 事务，无 leader compare；路径取得失败不写。 |
| `Member.GetMemberBinaryVersion` :477 | 此计划 2 | 直接读 binary_version，缺失同 no value 错误；不解释 semver。 |
| `Member.GetMemberGitHash` :490 | 此计划 2 | 直接读 git_hash，缺失同 no value 错误；不解释 hash。 |
| `Member.SetMemberBinaryVersion` :503 | 此计划 2 | 原样写 releaseVersion，单 Put 事务，无 leader compare。 |
| `Member.SetMemberGitHash` :517 | 此计划 2 | 原样写 gitHash，单 Put 事务，无 leader compare。 |
| `Member.Close` :531 | 服务主题 7 / 10 | 关闭 embedded etcd 的所有服务/listener；Rust 生命周期归宿主。 |
| `Leader.GetListenUrls` :543 | 服务主题 7 | Leader 包装返回 pdpb.Member.ClientUrls。 |
| `Leader.GetRevision` :548 | 服务主题 7 | Leader 包装返回读取时的 mod_revision；底层 revision 已有，包装未迁移。 |
| `Leader.String` :553 | 服务主题 7 | Leader 包装委托 protobuf String。 |
| `Leader.Watch` :558 | 服务主题 7 | Leader 包装委托 Member.WatchLeader。 |

#### participant.go：27 个导出函数

| 符号（源行号） | 归属 | 事实、已有原语与未覆盖原因 |
| --- | --- | --- |
| `NewParticipant` :70 | 服务主题 8 | 接收 client/MsParam，不执行 embedded etcd join。 |
| `Participant.InitInfo` :78 | 服务主题 8 | Marshal 服务 participant、保存 opaque value 并创建 Leadership；协议适配/初始化未实现。 |
| `Participant.ID` :91 | 服务主题 8 | 读取服务 participant.GetId。 |
| `Participant.Name` :96 | 服务主题 8 | 读取 participant.GetName。 |
| `Participant.GetMember` :101 | 服务主题 8 | 返回服务 participant 为 any。 |
| `Participant.MemberValue` :106 | 服务主题 8 | 返回服务 Marshal string。 |
| `Participant.ParticipantString` :111 | 服务主题 8 | nil 返回空串，否则服务 protobuf String。 |
| `Participant.IsExpectedPrimary` :123 | 服务主题 8 | 仅对 ListenUrls 做精确 slices.Contains，nil 为 false，不能用序列化 substring 代替。 |
| `Participant.Client` :131 | 服务主题 8 | 暴露 etcd 客户端；Rust 仍走 MetaStore。 |
| `Participant.IsServing` :137 | 服务主题 8 | Check 且 primary ID 为自身且 campaignCheck 成功；服务缓存/checker 未实现。 |
| `Participant.IsPrimaryElected` :142 | 服务主题 8 | primary ID 非零；未设置时工厂构造对应服务的零值 participant。 |
| `Participant.GetServingUrls` :147 | 服务主题 8 | 读取 primary.ListenUrls。 |
| `Participant.GetPrimaryID` :152 | 服务主题 8 | 读取 primary.ID。 |
| `Participant.PromoteSelf` :177 | 服务主题 8 | 发布自身为缓存 primary。 |
| `Participant.GetElectionPath` :182 | 服务主题 8 | 已有 ClusterPaths.election_path(Some(MsParam)) 路径原语；Participant 包装未迁移。 |
| `Participant.GetLeadership` :187 | 服务主题 8 | 已有 canonical Leadership；无 Participant 当前任期 getter。 |
| `Participant.Campaign` :192 | 服务主题 8 | 委托 CampaignWithCmps；Participant checker 和状态机未迁移。 |
| `Participant.CampaignWithCmps` :200 | 服务主题 8 | 先 campaignCheck，再追加 etcd cmps 原子选主；已有 campaign_with_key 的 Compare 原语，MCS 转换/策略未迁移。 |
| `Participant.CheckPrimary` :223 | 服务主题 8 | 读 proto+revision、自身残留则 revision 删除、错误 200ms 退避；仅底层 observer/delete 已有。 |
| `Participant.WatchLeader` :257 | 服务主题 8 | 设置 primary，观察退出再清空；仅底层 watch 已有。 |
| `Participant.Resign` :271 | 服务主题 8 | 先 unsetPrimary 再 Reset；未实现 MCS 缓存顺序，不能以底层 reset 测试替代。 |
| `Participant.SetCampaignChecker` :294 | 服务主题 8 | atomic 存储私有 leadershipCheckFunc；nil/错误动态类型按 campaignCheck 规则处理。 |
| `NewParticipantByService` :299 | 服务主题 8 | tso/scheduling/resource_manager 三种 protobuf 工厂；未知服务返回 nil，不能泛化恒成功工厂。 |
| `Primary.GetListenUrls` :320 | 服务主题 8 | Primary 包装返回服务 ListenUrls。 |
| `Primary.GetRevision` :325 | 服务主题 8 | Primary 包装返回读时 mod_revision。 |
| `Primary.String` :330 | 服务主题 8 | Primary 包装委托服务 protobuf String。 |
| `Primary.Watch` :335 | 服务主题 8 | Primary 包装委托 Participant.WatchLeader。 |

#### 类型、接口、多行签名与 Rust 公开面

- Go 导出类型 `Member`、`Leader` 归服务主题 7，`Participant`、`Primary` 归服务主题 8；
  Rust 同名 `Member` 仅为已有的不可变注册入口，不表示 Go mutable member 已移植。
- `election.go:26` 的导出 `Election` 接口归服务主题 7/8，共 11 个成员，逐项为
  `ID`、`Name`、`MemberValue`、`Client`、`IsServing`、`PromoteSelf`、`Campaign`、
  `Resign`、`GetServingUrls`、`GetElectionPath`、`GetLeadership`。
  每项行为分别对应上两表；Rust 没有该接口的直接移植，具体 client 类型留适配层。
- 私有成员也补读：Member 的 setLeader/unsetLeader/preCheckLeader/getPersistentLeader/
  isSameLeader；Participant 的 participant 接口（GetName/GetId/GetListenUrls/String/
  Marshal/Reset/ProtoMessage）、leadershipCheckFunc、getPrimary/setPrimary/unsetPrimary/
  getPersistentPrimary/isSamePrimary/campaignCheck。它们分别支撑上述服务状态机/协议适配，
  不形成未归属的存储缺口。atomic 缓存、时钟和 campaignChecker 不应混入持久 identity。
- `member.rs:17` 的 `pub struct Member`、`:25` 多行 `bootstrap` 和 `:66` `campaign`
  均标为已有；已补读完整参数和函数体，不按单行 rg 结果推断签名。
  `:6` 的 MemberError（ElectionError 别名）、Leadership、Result 重导出亦为已有；
  `lib.rs` 重导出 member 模块，注册独立 member_test。Leadership 的实现仅位于 election crate。
- `Member::bootstrap` 接受 Arc<dyn MetaStore>、CancellationToken、cluster_id、member_id、
  Vec<u8>，返回 Result<Member>；campaign 接受 i64 TTL，返回 Result<Leadership>。
  `cancellable` 为私有 helper，以 biased select 优先取消；没有私有 runtime。
- member crate manifest 仅依赖 storage-kv、election、tokio、tokio-util；无 features 表。
  没有 member_info/leader_priority 生产模块或测试注册，不能把 keypath 实现算作存储实现。

### key/value、事务、watch/lease 清单

Go 路径来自 [absolute_key_path.go](../../pkg/utils/keypath/absolute_key_path.go)、
[member.go](../../pkg/utils/keypath/member.go) 与
[election.go](../../pkg/utils/keypath/election.go)。Rust 已有
[ClusterPaths](../../pkg/utils/keypath/member_config_paths.rs)，cluster ID 为显式实例状态；
成员 ID 使用无补零十进制。后续应复用这些路径，不能另设全局 cluster ID。

| key（c=cluster_id，m=member_id） | value、事务和缺失语义 | lease/watch | 归属 |
| --- | --- | --- | --- |
| `/pd/{c}/rust/members/{m}` | opaque identity bytes；CreateRevision=0 写，冲突后 exact 同值可恢复、异值报错 | 无 lease、无专属 watch | 已有 bootstrap，不迁移旧数据 |
| `/pd/{c}/member/{m}/deploy_path` | 原始路径字符串；Go 从当前可执行文件取目录；单 Put，缺失读报 no value | 无 lease、无 watch | 此计划 2 |
| `/pd/{c}/member/{m}/binary_version` | 原样字符串；单 Put，缺失读报 no value | 无 lease、无 watch | 此计划 2 |
| `/pd/{c}/member/{m}/git_hash` | 原样字符串；单 Put，缺失读报 no value | 无 lease、无 watch | 此计划 2 |
| `/pd/{c}/member/{m}/leader_priority` | 十进制文本；读缺失 0、按有符号 32 位解析；Put/Delete 必须通过 LeaderTxn | 不把配置绑到 leader lease；无 watch | 此计划 3 |
| `/pd/{c}/leader` | Go pdpb.Member bytes；Rust adapter 提供 bytes；创建键原子选主；应用事务 fencing；自残留按 mod_revision 删除 | 绑定当前 lease；观察 exact key、取消、删除终止、压缩后快照 | 选主原语已有；PD 缓存/角色循环属服务主题 7 |
| `/ms/{c}/tso/00000/primary`；`/ms/{c}/tso/keyspace_groups/election/{group:05}/primary`；`/ms/{c}/{service}/primary` | TSO/其他服务 participant wire bytes；可附加原子 compare | lease + watch 复用 canonical election；服务工厂/重定向未移植 | 路径和选主原语已有；服务主题 8 |
| 上述 primary key 后接 `/expected_primary` | 转让 marker（advertise address），必须精确匹配 listen URL | 不是当前三个来源文件新增的持久写接口 | 消费/原子策略属服务主题 8 |

任务 2 的三个 setter 在 Go 各自独立事务提交，不保证三字段整组原子更新；
SlowLogTxn 的默认事务不附加 leader compare，不能为了复用 Leadership 而提高调用门槛。
已存在的空字符串与缺失必须区分；getter 取 value，不做版本/hash 格式校验。
事务/传输错误向上返回，Succeeded=false 的特定错误也不能吞掉。
任务 3 读不需要 leader；写/删的 LeaderTxn 比较 leader value。
Rust canonical Leadership.transact 还比较 lease 与 mod_revision 防 ABA，不能降为先读再写。
Go 的 int 入参经 Itoa 编码，但读取限制 32 位；负数允许、越界解析报错，不能擅自改成 u64。

Go WatchLeader/Participant.WatchLeader 本身仅负责缓存围绕 leadership.Watch 的生命周期；
重试、删除、压缩恢复和 reset 清理应复用 election 基线，不能新增第二套 MetaStore。
Member.Resign 先撤销可见身份再做可能阻塞的 Reset，此顺序由服务主题 7 接续。

### 反向依赖与工厂装配

| 消费者（均为现存路径） | 已核对调用与边界 |
| --- | --- |
| [server/server.go](../../server/server.go) | NewMember/InitMemberInfo，启动时写 deploy/version/hash；CheckLeader、Campaign、Keep、PromoteSelf、Resign、CheckPriority 组成 PD 角色循环。 |
| [server/api/member.go](../../server/api/member.go) | 成员列表读取四种元信息；成员删除清理 priority；API 修改 priority。HTTP 状态/JSON、etcd 成员删除与 API 适配归服务主题 7。 |
| [pkg/tso/allocator.go](../../pkg/tso/allocator.go)、[tso.go](../../pkg/tso/tso.go) | 消费 member.Election、IsServing、GetLeadership、PromoteSelf、Resign；不能将 Rust 返回 Leadership 等同整个 Election 接口。 |
| [keyspace_group_manager.go](../../pkg/tso/keyspace_group_manager.go) | NewParticipant 与 Election，服务/group 选主策略属主题 8。 |
| [MCS TSO server](../../pkg/mcs/tso/server/server.go)、[scheduling server](../../pkg/mcs/scheduling/server/server.go)、[resource manager server](../../pkg/mcs/resourcemanager/server/server.go) | Go Participant 工厂/接口消费者，主题 8 分服务移植。 |
| [pkg/runtime/node.rs](../../pkg/runtime/node.rs) | initialize_node 由协议适配处 encode pdpb.Member，再 bootstrap/campaign；这是已有最小装配，尚未调用发布信息或优先级接口。 |
| [pkg/id/allocator.rs](../../pkg/id/allocator.rs)、[pkg/tso/oracle.rs](../../pkg/tso/oracle.rs) | 通过 member 重导出使用 canonical Leadership/MemberError，不拥有第二套选主实现。 |
| [pkg/lib.rs](../../pkg/lib.rs) 与根 manifest | 根 facade 只重导出 runtime API，间接使用 member；直接 Cargo 反向依赖为 id、tso、runtime，相关测试复用 bootstrap。 |

Go `InitMemberInfo` 是 PD protobuf 工厂；`NewParticipantByService` 是三个 MCS protobuf
工厂。Rust 对应协议编码已在 runtime adapter，member 领域 crate 不依赖 proto。
具体工厂替代、服务重定向、etcd join、发现、指标和退出生命周期不纳入任务 2/3。
发布信息的进程路径获取应由宿主/适配边界提供，不能在领域库安装全局状态或私有 runtime。

### 相邻测试与证据等级

| 现存证据 | 可证明/待补项 |
| --- | --- |
| [member_test.rs](../../pkg/member/member_test.rs) | memory_bootstrap_and_lease_loss 与 behavior：同值恢复/冲突、竞争选主、lease/revision、续租、撤销、旧任期拒绝写入和取消；memory_natural_expiry 与 expiry：自然过期后 fencing/重选。源码已复核，本轮不重跑。 |
| [etcd_behavior_test.rs](../../pkg/member/etcd_behavior_test.rs) | real_etcd_behavior / real_etcd_natural_expiry 复用同一断言；real_etcd_identity_recovery 在 stop/start 同 WAL 后验证异值冲突、同值恢复并重选。manifest 显式注册 etcd_behavior；通过 path 引入 scenario 也会注册两个 memory 测试，不能把重复测试 target 算成新增语义。 |
| [watch_reset_test.rs](../../pkg/election/watch_reset_test.rs) 及 [选主证据](theme3-存储选主.md) | 批次 66 双后端压缩恢复和 reset 任期竞态已有真实运行证据；不证明 PD/MCS 内存身份发布顺序已经装配。 |
| [member_config_paths_test.rs](../../pkg/utils/keypath/member_config_paths_test.rs) | 已保存 Go oracle 输出，member_config_paths_go_oracle / independent_contexts 覆盖路径和独立 cluster；只证明路径，不证明任务 2/3 的存储、失败与取消。 |
| [tests/server/member/member_test.go](../../tests/server/member/member_test.go) | TestLeaderPriority 通过 API 将旧 leader priority 设为 -1 并观察转让；TestPDLeaderClearsIdentityBeforeBlockingCleanup 断言 GetLeader=nil、lease/value 清理；另有 lease 丢失/续租失败/step-up 发布时序测试。只复用源码，不宣称本轮 Go 运行。 |
| [tests/server/api/member_test.go](../../tests/server/api/member_test.go)、[pd-ctl member tests](../../tools/pd-ctl/tests/member/member_test.go) | 成员列表、leader URL/ID、修改优先级等 API/工具级断言；当前缺少面向 Rust 六个发布信息接口及三种 priority 操作的跨后端独立测试。 |
| [MCS member tests](../../tests/integrations/mcs/members/member_test.go) | 服务成员/primary 转让、lease 到期、keyspace group 场景；留主题 8，不能用 PD identity 测试代替。 |
| [health_member_test.rs](../../tests/rust-compat/health_member_test.rs)、[runtime node_test.rs](../../pkg/runtime/node_test.rs) | runtime 测试有最小成员请求；compat 无 PD_COMPAT_MODE 时运行示例进程，有环境配置才可运行真实 oracle；不能据其存在声称真实差分通过，亦未覆盖发布信息/优先级。 |

本轮执行的是源码与历史证据审查；没有运行 Go/Cargo、fixture 构建或生成器，
不声称新 Go/Rust 运行差分、编码差分或双后端存储验收。
已有 [wire-fixtures.json](../../pkg/proto/wire-fixtures.json) 的 members_response 保存
Go gogo 真实输出（输入见 [既有 oracle](../../scripts/rust-proto-oracle/main.go)），
但不是任意新 Member/Participant 输入的完整 codec 证据。任务 2/3 写的是原始字符串/数字文本，
不应顺便实现 wire codec；如后续新增 codec 场景，必须复用匹配的只读 Go 输出，
缺失则记录阻塞，不能从 Rust 输出倒填预期。

后续验证缺口已全部归属：任务 2 覆盖缺失/空值、多个成员隔离、取消、写失败及三个独立字段；
任务 3 覆盖缺失默认、负值、非法/溢出、读写删除、成员隔离与写/删 fencing。
两者都须 MemoryStore 与真实 etcd 共用状态断言及实际红绿证据；
尚未存在的实现/测试路径仅为拟建（本轮不创建）：

```text
pkg/member/member_info.rs                 # 拟建，任务 2
pkg/member/member_info_test.rs            # 拟建，任务 2，显式注册
pkg/member/leader_priority.rs             # 拟建，任务 3
pkg/member/leader_priority_test.rs        # 拟建，任务 3，显式注册
```

### 本轮检查与完成证据

| 检查 | 结果 |
| --- | --- |
| `rg -n '^(func |type |pub |    pub )' pkg/member/member.go pkg/member/election.go pkg/member/participant.go pkg/member/member.rs` | 退出 0；补读 interface 成员、多行签名和私有辅助函数，覆盖 40+27 个导出 Go 函数、5 个导出 Go 类型及 Rust 公开面。 |
| `git diff 5d5948052a9ee9e3fa7a09145fe2916a2912c223 HEAD -- pkg/member/member.go pkg/member/election.go pkg/member/participant.go pkg/member/member.rs` | 退出 0；三份 Go 无差异，Rust 变化与上述 canonical 抽取一致。 |
| 只读调用方/manifest/测试检索与人工源码对照 | 所有接口均标为已有、此计划 2/3 或服务主题 7/8/10，无未归属缺口。 |
| Python 校验 Markdown 相对链接及公开符号清单 | 退出 0；链接目标均存在；拟建四文件单独标注；67 个 Go 导出函数和 11 个 Election 方法逐项覆盖。 |
| Python 对照开始/结束 Go/模块路径和 SHA-256 | 退出 0；1101 个文件完全相同，Go/模块 Git 状态为空。 |
| `git diff --check`、`git diff --cached --check` | 退出 0；审查内容仅本任务证据文档，编号任务按完成流程删除。 |

没有运行时行为变化，TDD 不适用，也不调用 Cargo 或占用共享构建槽位。
总计划、来源主题、Go/模块、Rust/Cargo、前序证据均保持只读；
其他任务已有状态修改与删除保留并排除提交。本轮完成的是盘点，
不表示任务 2/3、服务主题或 embedded etcd 生产替代已完成。

## 任务 2：成员发布信息（批次 68）

日期：2026-10-23（Asia/Shanghai）。准入依据：批次 67 提交 `2306b1d02`，
以及上文完整盘点；前置文档删除不作为完成依据。当前 Go `pkg/member/member.go`
相对规划基准仍无差异。复核 `GetMemberDeployPath` / `SetMemberDeployPath`
（445–474）、`GetMemberBinaryVersion`（477–487）、`GetMemberGitHash`
（490–500）、`SetMemberBinaryVersion`（503–514）、`SetMemberGitHash`（517–528）。
三个 getter 缺失报 no value；setter 单 Put、无比较、无 lease，原始值不做解释。
相邻 Go member 源码有 `skipCampaignLeaderCheck` / `staleEtcdLeaderView`
failpoint；未执行 `go test -mod=readonly ./pkg/member -count=1` 或任何 failpoint 命令。
本轮 Go 依据为只读源码和批次 67 的既有证据，没有声称新增 Go 运行差分。

实现 [member_info.rs](../../pkg/member/member_info.rs) 的 `MemberInfo`：六个独立
读写接口，复用 `ClusterPaths` 与唯一的 `MetaStore`，不依赖 identity bootstrap
或 leadership。exact key 线性读，缺失返回 `MemberInfoError::NoValue`，现存空值有效；
存储错误保留类型，事务不成功映射为带字段的 SaveFailed，取消优先。
使用字节承载 Go string，保留非 UTF-8/NUL；本任务不新增 codec，不编码 protobuf，
也不从 Rust 结果倒填 Go fixture。路径已有真实 Go capture 位于
[member_config_paths_test.rs](../../pkg/utils/keypath/member_config_paths_test.rs)。
Go 直接保存字符串的结论来自源码，不冒充新增运行编码证据。
部署目录由宿主先解析当前 executable 再提供，解析失败由宿主返回且不调用写接口；
领域库不自行读取进程环境，也不新增服务启动装配。

独立 [member_info_test.rs](../../pkg/member/member_info_test.rs) 在 lib.rs 显式注册。
`member_info_memory` / `member_info_etcd` 使用同一行为函数，覆盖三个字段的
缺失/空值、原始值、路径与无 lease、相邻 prefix 排除、覆盖、成员与集群隔离，
以及预取消后的全部六接口与最终存储不变。
`member_info_etcd_io_failure_and_cancellation` 在三字段写入确认后停止 etcd，
验证三个 setter 与 getter 返回 Unavailable，再以同 WAL 重启验证三个旧值均保留。
`member_info_pending_io_cancel` 使用真实 loopback TCP 接收端抑制 HTTP/2 回复：
读请求已轮询并等待 50ms 后取消，500ms 内终止；随后关闭并等待 TCP 任务。
最初用已断开的 etcd 连接验证在途取消会立即返回 Unavailable，未能构造在途阶段；
因此改为受控网络 IO 边界，没有模拟 MetaStore 事务结果。

共享槽位动态原子领取：槽位 **1**，持有 shell PID **69377**；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
所有 Cargo 与 fixture 子进程继承此绝对路径；Go fixture 仅构建既有源码，
设置 `GOFLAGS=-mod=readonly`，不运行 generator、Go 格式化或 failpoint 注入。
Cargo.lock 仅增加 member 对 workspace 已有 keypath 和 thiserror 2.0.21 的引用。

| 本轮命令（仓库根目录） | 退出码及证据 |
| --- | --- |
| `cargo fmt --all` | 0；仅格式化本任务 Rust 文件，无其他 Rust 差异。 |
| `cargo check --workspace` | 0；同步上述两条既有依赖引用，未新增依赖版本。 |
| `cargo test --locked -p astersql-pd-member --lib member_info -- --nocapture`（红） | 101；4 个有效测试，2 passed / 2 failed。两后端均因缺失键实际 `Ok([])` 与 `Err(NoValue)` 断言不符失败；不是缺符号或环境失败。 |
| 同一聚焦命令（绿） | 0；同一 4 个测试全部 passed / 0 failed / 0 ignored；仅修改 load 将缺失与空值区分，未削弱测试。 |
| `cargo metadata --format-version 1 --no-deps` | 0。 |
| `cargo fmt --all -- --check` | 0。 |
| `cargo check --workspace --locked` | 0；保留既有 server deprecated 字段警告。 |
| `cargo test --workspace --locked`（首次） | 101；已执行 610 passed / 1 failed / 1 既有 ignored 后停止。失败为既有 `astersql-pd-tso --test etcd_behavior` 的 `real_etcd_behavior`，`pkg/tso/oracle_test.rs:32` bootstrap 返回 `Store(Unavailable)`；TSO/fixture 源码无本任务差异。本任务 4 个测试在该轮全部通过。 |
| `cargo test --locked -p astersql-pd-tso --test etcd_behavior real_etcd_behavior -- --exact --nocapture` | 0；1 passed / 0 failed；不修改无关代码，复跑确认该目标可通过。 |
| `cargo test --workspace --locked`（完整复跑） | 0；633 passed / 0 failed / 1 既有 ignored（含 doc-test 汇总为 85 个 target 结果，行为有效测试数为 633）；没有跳过失败 target 或削弱断言。 |
| Go/模块文件开始与结束的路径及 SHA-256 对照 | 0；1101 个文件完全一致，相关 Git 差异为空。 |
| Markdown 相对链接校验、`git diff --check`、`git diff --cached --check` | 0；仅审查/提交本任务实现、测试、最小依赖连线及本文追加段落。 |

状态：已完成。全部 Cargo/fixture/TCP 子进程结束后，已清理 PID 69377 持有的
`target/rust-slot-locks/slot-1.lock`，保留 `target/rust-slot-1` 缓存。
没有修改总计划、来源主题、Go/模块、TSO 或其他任务的源码；
工作树原有编号任务状态修改/删除保留且排除提交。没有新增 ignored 测试。

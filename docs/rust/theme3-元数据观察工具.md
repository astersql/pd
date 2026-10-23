# 元数据观察工具覆盖盘点

## 任务 1：消费者与选项（批次 74）

日期：2026-10-24（Asia/Shanghai）。状态：已完成（仅盘点）。
规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223`；审查 HEAD
`2850f1f7307e8398d699f3964dcccf419805ce34`。
已读仓库 AGENTS.md、Copilot 指南、[ADR-001](architecture.md)、本任务及只读计划。
仅根 AGENTS.md 适用。未修改 Go、模块文件、生产代码、只读 plan.md 或来源主题。

### 准入与证据性质

批次 72 提交 `264a516f213201f81d7fbff7a9429248b2169a81`、批次 73 提交
`2850f1f7307e8398d699f3964dcccf419805ce34` 均为当前 HEAD 祖先。
已核验[前置证据任务 3/4](theme3-标识分配补齐.md)、
[namespace 类型](../../pkg/id/allocator_namespace.rs)、
[原位构造入口](../../pkg/id/allocator.rs)、
[独立测试](../../pkg/id/allocator_namespace_test.rs) 与
[显式注册](../../pkg/id/lib.rs)。批次 73 保存了真实行为红灯
1 passed / 2 failed（两个后端串用全局键，1001 != 1）、绿灯 3 passed、
workspace 643 passed / 0 failed / 1 既有 ignored，包含隔离、上界、旧任期拒绝及
同 WAL 重启恢复；槽位及 Go 只读校验证据完整。因此旧阻塞已解除，未以编号文件删除
代替完成证据。本轮没有重新运行这些测试。

以下行为全部是本轮只读源码与断言审查；不是 Go/Rust 新运行差分，也不宣称已迁移。
文档任务不运行 Go、Cargo、make、failpoint 或生成器，不需要 Cargo 槽位。
主来源 [etcdutil.go](../../pkg/utils/etcdutil/etcdutil.go) 相对规划基准无差异。

### 主文件符号全覆盖与归属

行号均固定于审查 HEAD；多行 NewLoopWatcher、EtcdKVGetWithContext、reloadWithRetry
签名以及方法体已补读。任务 2–6 分别为本计划的一致分页快照、快照接续观察、
压缩重载与删除对账、加载控制、限时键写入（批次 75–79）；上层主题指渐进迁移主题 3。

| 符号 / 行号 | 事实与归属 |
| --- | --- |
| DefaultDialTimeout、DefaultRequestTimeout、DefaultSlowRequestTime、MaxEtcdTxnOps / 59–74 | 30s、10s、1s、120；连接/日志/事务阈值不是本主题的新配置。上层客户端与事务契约；加载使用 10s 超时，任务 2/5 处理宿主取消。 |
| CheckClusterID、AddEtcdMember、ListEtcdMembers、ListEtcdMembersLinearizable、RemoveEtcdMember / 79–160 | etcd 集群身份及成员管理；后两种 list 分别本地读/线性读。全部范围外，归上层启动及 etcd 运维装配。私有 queryEtcdMembers 同归属。 |
| EtcdKVGet、EtcdKVGetWithContext / 163–195 | Get 加默认超时、错误封装及慢日志；后者尊重传入 ctx。任务 2 通过唯一 kv::MetaStore 使用等价读能力，不移植具体 client 类型。 |
| GetValue、GetProtoMsgWithModRev / 233–271 | 私有 get 检查零或恰好一个 KV，多值报错；前者返回原字节，后者 proto 解码并返回存在性/mod revision。领域值读取复用 KV，协议解码归上层编码适配器，不能向工具领域暴露 protobuf。 |
| EtcdKVPutWithTTL / 275–284 | Grant 后带 lease Put；Grant 失败返回 (0,error)，Put 失败仍返回已授 lease ID，无 revoke。任务 6；清理失败 lease 是任务目标的安全强化，不能声称 Go 已清理。 |
| EtcdClientPurpose、TestEtcdClientPurpose、ServerEtcdClientPurpose、ElectionEtcdClientPurpose、McsEtcdClientPurpose / 293–304 | 四种 client 用途标识；上层客户端健康检查与指标，范围外。 |
| CreateEtcdClientOpt、CreateEtcdClient、CreateHTTPClient / 307–366 | config 修改回调、client 创建/健康检查及 HTTP TLS transport；上层网络装配，范围外。私有 newClient 同归属。 |
| RequestProgressInterval、WatchChTimeoutDuration / 374–376 | 1s 请求 progress，默认 10s 无响应重建；任务 3 的断流恢复依据，宿主控制时钟与取消。 |
| LoopWatcher、NewLoopWatcher / 380–480 | 私有状态、四种事件回调及初次成功/重试钩子；构造接受宿主 ctx/wg/client、name/key、prefix bool。默认重试 3、batch 10000、watch retry 1s；开关默认 false，channel 各容量 1。任务 2–5，不复制全局 runtime。 |
| StartWatchLoop / 483–517 | wg 管理子协程；初始化后循环 watch，错误按固定间隔重启；退出依赖宿主 ctx。任务 3/5。 |
| ForceLoad、WaitLoad / 1004–1033 | ForceLoad 无返回确认，1s 限流且单槽非阻塞合并；构造时即记录时间，因此首秒可被抑制。WaitLoad 消费一次初始化结果，不是可重复共享 future，也不等待 ForceLoad。任务 5。 |
| SetLoadRetryTimes、SetLoadBatchSize / 1036–1043 | 直接赋值，无校验；生产 TSO group 条件为 >0 才调用。任务 2/5；不要从命名推断零或负数会被拒绝。 |
| SetConsistentLoad、SetAtomicLoadCallbacks / 1047–1060 | 前者固定 revision；后者只在 consistent 模式阻止失败/未完加载调用 post，要求消费方暂存并成功发布。不是自动回滚所有回调副作用。任务 2。 |
| SetInitialLoadSuccessFn、SetInitialLoadRetryFn / 1064–1074 | 成功钩子在初次 load 成功且 ctx 未取消后、通知 WaitLoad 前调用；重试钩子仅首次失败后的 i>0 使用，并返回下一 watch revision。任务 5，快照恢复部分任务 4。 |
| SetReloadOnCompaction、SetCompactionReloadFn / 1082–1093 | 前者启用一致全量重载；后者直接启用自定义替代重载，不自动打开 live post 重试。任务 4。 |
| SetRetryOnPostEventError / 1102–1104 | post 失败不推进；有 custom 则最新快照对账，否则原 revision 重放，优先于 atomic 全量重载。任务 3/4。 |
| SetReconcileDeletedKeys / 1110–1114 | 初始化 loadedKeys 并启用压缩重载；空间 O(已成功应用键)，仅建议有限基数。任务 4。 |
| 私有 initFromEtcd、watch / 519–752 | 初始化有限重试；从 load 返回的 nextRevision 观察，含 progress、断流、ForceLoad、compaction 和 callback 路由。任务 3/5。 |
| 私有 reloadAfterCompaction、reloadWithRetry / 754–795 | custom 优先；失败指数退避，上限 1min，直到成功或取消；成功值已是 next revision。任务 4。 |
| 私有 load、buildLoadingOpts / 797–952、979–1001 | 固定范围上界、字节序升序、limit+1 游标、limit=0 无限；资源耗尽/超时减半至最小 100（0 先变 10000）；consistent 首页固定 revision。任务 2。 |
| 私有 reconcileLoadedKeys / 954–977 | 对旧 loadedKeys 中新快照无的键调用 key-only delete，收集成功项与首错；顺序来自 map，不承诺稳定。任务 4。 |
| 私有 writeKeyToFile / 198–216 | failpoint 测试工具，去数字后追加文件；不进入 Rust 生产库，归 Go 测试设施，保持只读。 |

其余私有常量为 client timeout/keepalive、healthyPath、健康巡检超时、重试/分页阈值，
已经分别归入上述网络范围外项目与任务 2–5，未保留未归属符号。

### 真实调用方与六项选项矩阵

记 C=一致分页，A=atomic callbacks，D=删除对账，R=compaction reload，
P=live post 失败重试；“默认”表示未调用相应 setter，不等于已满足强一致/原子发布。
D 同时隐式开启 C/R；R 隐式开启 C，custom R 本身不隐式开启 C/P。
所有行都使用通用初始化重试（默认 3 次、1s）与通用断流重启；下表“重试”列只列额外设置。
所有列缺省的 ForceLoad 均为“未发现生产调用”；未设置 A 的消费者可能已有锁或 batch，
不能据此冒称已打开工具的 atomic 开关。共有 17 个生产构造点，含 1 个可替换工厂。

| 调用方（文件:行） | key / value / 回调发布 | C / A / D / R / P | 额外重试、ForceLoad / WaitLoad；任务映射 |
| --- | --- | --- | --- |
| [router meta](../../pkg/mcs/router/server/meta/watcher.go):105 | StorePathPrefix，metapb.Store；Put/删除 BasicCluster，清理统计 | C/D/R，A/P 默认 | Start 后 WaitLoad；2/3/4/5，业务统计归上层 |
| [scheduling meta](../../pkg/mcs/scheduling/server/meta/watcher.go):139 | 同前；额外 tombstone 清理钩子 | C/D/R，A/P 默认 | gRPC GetStoreWatcher().ForceLoad；Start/WaitLoad；2/3/4/5 |
| [affinity group](../../pkg/mcs/scheduling/server/affinity/watcher.go):104 | AffinityGroupsPrefix 去尾斜杠，JSON Group；同步 manager | C/D/R，A/P 默认 | Start/WaitLoad；2/3/4/5 |
| 同文件 affinity label:144 | RegionLabelKeyPath(affinity.LabelRuleIDPrefix) 去尾斜杠，label rule JSON；内部锁同步范围 | C/D/R，A/P 默认 | Start/WaitLoad；2/3/4/5 |
| [scheduling config](../../pkg/mcs/scheduling/server/config/watcher.go):165 | ConfigPath 精确键，配置 JSON；直接更新配置，delete no-op | C/R，A/D/P 默认 | Start/WaitLoad；2/3/4/5；完整 Config/PersistOptions 属上层配置契约 |
| 同文件 TTL config:199 | TTLConfigPrefix 前缀，string value + kv.Lease；TimeToLive 后本地 TTL cache | C/D/R，A/P 默认 | Start/WaitLoad；2/3/4/5/6，值 lease 缺失/错误不能吞掉 |
| 同文件 scheduler config:249 | SchedulerConfigPathPrefix 去尾斜杠，raw JSON；存储及 reload controller | C/D/R，A/P 默认 | Start/WaitLoad；2/3/4/5；调度算法属上层 |
| [placement rule](../../pkg/mcs/scheduling/server/rule/watcher.go):538 | RuleCommonPathPrefix，Rule/RuleGroup JSON；pre 锁及 patch，post 检查 applyFailed 后提交 | C，custom R/P；A/D 默认 | InitialLoadSuccessFn 更新 ruleRevision；InitialLoadRetryFn 与 custom R 同用 reconcileRuleSnapshot；Start/WaitLoad；2/3/4/5，规则语义归上层 |
| 同文件 region label:611 | regionLabelPathPrefix 去尾斜杠，label JSON；pre 锁，post 解锁并加 suspect ranges | C/D/R，A/P 默认 | Start/WaitLoad；2/3/4/5 |
| [keyspace meta](../../pkg/mcs/scheduling/server/keyspace_meta/watcher.go):136 | KeyspaceMetaPrefix，keyspace 元数据；缓存更新、post 添加 suspect ranges | 全部默认 | Start/WaitLoad；2/3/5 默认兼容分支；不要自动替该消费者启用 4 |
| [资源管理工厂](../../pkg/mcs/resourcemanager/server/metadata_watcher.go):70、调用:166 | ResourceGroupPrefix；controller/settings/states/service limit，JSON/raw 字符串，由路径分派 | 全部默认，接口仅 Start/WaitLoad | 工厂可被测试替换；Start/WaitLoad 成功后 initReserved；2/3/5，资源组处理归上层 |
| [PD keyspace manager TSO nodes](../../pkg/keyspace/tso_keyspace_group.go):576 | ServicePath(TSOServiceName)，ServiceRegistryEntry JSON；nodesBalancer/map | C/D/R，A/P 默认 | init 构造，管理循环另行 Start，不调用该实例 WaitLoad；2/3/4/5 |
| 同文件 group reconcile:651 | KeyspaceGroupIDPrefix，KeyspaceGroup JSON；pre 暂存 snapshot/updates，post state.replace/apply | C/A/R，D/P 默认 | Start/WaitLoad；2/3/4/5，失败整批不发布依赖 batchErr 与 staging |
| [TSO manager nodes](../../pkg/tso/keyspace_group_manager.go):513 | ServicePath(TSOServiceName)，registry JSON；tsoNodes/map | C/D/R，A/P 默认 | Start/WaitLoad；2/3/4/5 |
| 同文件 group:592 | KeyspaceGroupIDPrefix 去尾斜杠，KeyspaceGroup JSON；更新 group、post retry list | C；A/D/R/P 默认 | 正数覆盖 loadRetryTimes/loadBatchSize；成功钩子设最大 mod revision；Start/WaitLoad，失败 Close；2/3/5 |
| [PD service primary 工厂](../../server/server.go):2510 | primaryKey 精确键，Participant proto；更新/删除 servicePrimaryMap | C/D/R，A/P 默认 | TSO/scheduling/resource-manager 各实例 Start，无 WaitLoad；TSO 在 forward/server/dispatcher 触发 ForceLoad；2/3/4/5，proto 解码归适配器 |
| [WatchKeyspaces](../../server/keyspace_service.go):151 | KeyspaceMetaPrefix，KeyspaceMeta proto；put 累积，post stream.Send；delete no-op | 全部默认 | Start/WaitLoad；发送/解码失败 cancel；2/3/5，gRPC 事件契约归上层，不能随意增加补发删除 |

ForceLoad 额外来源穷举：[forward.go](../../server/forward.go):256/542、
[server.go](../../server/server.go):2432、
[tso_dispatcher.go](../../pkg/utils/tsoutil/tso_dispatcher.go):163（TSO primary），
[scheduling grpc_service.go](../../pkg/mcs/scheduling/server/grpc_service.go):269（store）。
测试中 affinity/rule 的 ForceLoad 是独立测试使用，不是生产启用。
TTL 写唯一生产调用为 [persist_options.go](../../server/config/persist_options.go):955，
`TTLConfigPrefix + "/" + key`、string value、ttl.Seconds 转 int64；任务 6。
所有 key 构造的权威来源是 [keypath](../../pkg/utils/keypath/absolute_key_path.go)，
原始前缀有无尾斜杠及 false 精确键须原样保留，不能只按名称推断。

### key/value、事务与恢复细节

- LoopWatcher 无写事务，不拥有 lease；读取 KV 的原始 key/value、mod revision、lease，
  watch 删除通常只有 key。lease 写与清理仅任务 6；不复制第二套 MetaStore。
- 页排序固定字节序，prefix range end 从原始 key 计算，不能每页改用新 cursor 的 prefix end。
  limit+1 的末项留作下一页 inclusive start，只处理前 limit 项；不限量和空前缀均需验证。
  consistent 用第一页 header revision 固定后续读取，返回该 revision+1；默认每页 latest，
  返回最后页 header+1，可能跨 revision。任务 2 必须区分默认兼容与目标强保证。
- load 的 pre/post 收空 events，watch 收实际 events。默认模式即使 callback 失败也继续，
  最后 put 错误可能覆盖前错，post 错误仅日志；consistent 保留首个 put 错误并上返 pre/post。
  atomic 需要 C，跳过失败/未完成 full-load 的 post；取消可能让 load 返回 (0,nil)，
  不能只按 err==nil 判断快照已成功。任务 2/5 要核验终态而非仅编译。
- 已成功加载/提交的键才进入 loadedKeys。新完整快照缺失的旧键补 key-only delete；
  非 atomic 模式部分成功且 post 成功时，已成功 put/delete 也要记账，避免非幂等 delete
  被重试两次。post 失败则不提交账本；watch 账本亦在 post 成功后更新。任务 4。
- 默认 compaction 直接跳 CompactRevision，不重建历史；启用 R/D 则重载；custom R 优先。
  重载失败按 1s 起（watchChangeRetryInterval）倍增至 1min，取消终止，不得在失败时推进。
  A 下 live post 失败触发 full load；P 优先，custom 则对账，无 custom 则原 revision 重放。
  仅 pre/put/delete 返回错而 post 不报错时仍可能推进，故消费者必须正确汇总 batchErr。
- watch 请求含 RequireLeader、inclusive revision、ProgressNotify；progress 取 max(old,header+1)，
  事件成功后 header+1。断流/超时重新建 watch，禁止丢失 snapshot 与 watch 建立间隙的事件。
  ForceLoad 成功采用 load 返回的 next revision，失败保留原 revision。任务 3/4/5。
- TTL helper 的 Put 不确定失败可能已经提交；Go 不清理 lease。本计划任务 6 明确要求失败
  不泄漏有效 lease，需在真实后端故障证据中说明 revoke 与已绑定键删除的安全取舍，
  不得盲重试写或将“Go 未 revoke”隐藏为已一致。

### 相邻 Go 测试依据（只读，未运行）

主来源 [etcdutil_test.go](../../pkg/utils/etcdutil/etcdutil_test.go)，
TestLoopWatcherTestSuite 在 :421 注册。以下覆盖所有 suite 行为方法，SetupSuite、
TearDownSuite、startEtcd、put 是 fixture。断言是已有源码事实，不是本轮通过结果。

| 测试（省略 Test 前缀） | 已有断言 / 归属 |
| --- | --- |
| LoadNoExistedKey、WatcherLoadInitializesEmptyReconciliationSnapshot | 空加载无错，cache/loadedKeys 为空；2/5。 |
| InitialLoadFailsWhenContextCanceled、WatcherLoadCancelsInFlightRequest | WaitLoad 报错、在途读尊重 context.Canceled；2/5。 |
| WatcherLoadReturnsLifecycleErrors、WatcherLoadKeepsDefaultCallbackSemantics | consistent pre/post 错上返，默认保留旧 callback 语义并调用一次 post；2。 |
| LoadWithLimitChange、WatcherLoadLimit、WatcherLoadLargeKey | 自适应 batch、不同 limit、超大 key 完整加载；2。前者使用 failpoint，本轮禁止执行。 |
| CallBack | put/delete 回调后缓存增加/清空；3。 |
| WatcherLoadPreservesFirstCallbackError | consistent 返回 firstErr，仍处理三项；2。 |
| WatcherConsistentLoadUsesSingleRevision | 加载中改值仍全部为 old，返回 snapshotRevision+1；2/3。 |
| WatcherAtomicLoadCallbacksDiscardIncompleteLoad | 取消/未完成时不 published；2。 |
| WatcherLoadKeepsDefaultPaginationAcrossCompaction | 默认跨压缩分页继续，post 一次且有三项；不得冒称一致快照；2 默认边界。 |
| WatcherBreak、WatcherRequestProgress | 断流换 client、ForceLoad 恢复、阻塞 channel 后 progress/重建继续；3/5，含 failpoint。 |
| WatcherReloadsAndReconcilesAfterCompaction、WatcherSkipsCompactionReloadByDefault、WatcherReloadsAfterCompactionWhenEnabled | D 恢复并删除旧键；默认不 full reload；R 获得最新值；4。 |
| WatcherUsesCustomCompactionReload、CustomCompactionReloadDoesNotEnableLivePostRetry | custom 替代全量重载，但不隐式打开 P；4。 |
| WatcherDoesNotAdvanceAfterLivePostCallbackFailure、WatcherRetriesLivePostCallbackFailure | P 返回未推进 revision，重试后 committed=updated、失败一次；3/4。 |
| WatcherStopsCompactionReloadWhenContextCanceled、WatcherRetriesFailedCompactionReloadWithBackoff | 取消停止、不会额外 put；失败重载三次且间隔退避；4/5。 |
| WatcherRetriesReconciliationAfterPostCallbackFailure、WatcherCommitsPartialReconciliationProgress | post 失败缓存仍旧，重试后空；部分删除成功须记账，后续不重复删除；4。 |
| WatcherRetriesWatchDeleteAfterPostCallbackFailure、WatcherReloadsFailedAtomicWatchBatch | watch delete post 失败后重载收敛；atomic 失败批不发布，再以 authoritative full load 恢复；3/4。 |
| EtcdKVPutWithTTL（非 suite） | 两键分别 TTL 2s/4s 后删除；无 Put 失败 lease 清理断言，缺口任务 6。 |
| EtcdKVGet（非 suite） | limit/range/sort/cursor 读；2 底层依据。 |

其他非 suite 测试 AddMember、MemberHelpers、EtcdClientSync、EtcdScaleInAndOut、
RandomKillEtcd、EtcdWithHangLeaderEnableCheck 与三个 WriteKeyToFile 测试，分别归范围外
客户端/健康/运维与 Go 测试工具，不作为本主题行为验收。
消费者相邻测试路径为各 watcher.go 的 watcher_test.go，以及
[metadata_watcher_test.go](../../pkg/mcs/resourcemanager/server/metadata_watcher_test.go)；
其中 rule/affinity 的 ForceLoad 用于缓存收敛测试。具体业务算法不纳入本次迁移。

### 现有 Rust 能力与精确缺口

[canonical KV](../../pkg/storage/kv/lib.rs) 与
[MetaStore](../../pkg/storage/kv/meta_store.rs) 已提供 Read 的 prefix/start/end/limit/revision、
Snapshot.more/header revision、MetaWatch/WatchBatch、compaction 错误、grant/revoke/ttl；
内存和 etcd 为同一个领域契约。Snapshot.revision 是 header revision，历史读取也可能返回
更新的 header；续页必须保留首个选定 revision，不能每页从返回 header 改写。
[contract_test.rs](../../pkg/storage/kv/contract_test.rs) 的 snapshot_scan、watch_replay、
compaction_recovery、lease_expiry、lease_renew_revoke、watch_cancel 已有真实断言；
[meta_store_test.rs](../../pkg/storage/kv/meta_store_test.rs) 显式注册内存入口，
[etcd_store_test.rs](../../pkg/storage/kv/etcd_store_test.rs) 复用并覆盖断连/WAL 重启；
[bounded_range_test.rs](../../pkg/storage/kv/bounded_range_test.rs) 有双后端范围断言。
这些底层测试没有实现消费者 staging、完整扫描循环、删除账本或 ForceLoad/WaitLoad。

[election LeaderObserver](../../pkg/election/watch_reset.rs) 已实现精确 leader 键的
snapshot→revision+1→watch、压缩重新 snapshot，
[watch_reset_test.rs](../../pkg/election/watch_reset_test.rs) 与
[leader_watch_test.rs](../../pkg/election/leader_watch_test.rs) 覆盖建立间隙、邻键过滤、
压缩、断流和取消；[election lib.rs](../../pkg/election/lib.rs) 显式注册。
这是选主单键观察能力，不能冒称通用 prefix LoopWatcher；本主题复用 KV 能力，
不把选主 observer 复制成第二套 MetaStore。

当前 pkg/utils/etcdutil 没有 Rust 文件或 Cargo.toml，根 workspace 无 etcdutil member，
也没有通用工具的 Rust 工厂装配/测试。以下均为拟建，未将缺失路径作已存在链接：

| 缺口 | 唯一任务归属与拟建路径 |
| --- | --- |
| canonical crate、固定 revision 分页、完整成功后发布、取消/空范围 | 2：pkg/utils/etcdutil/{Cargo.toml,lib.rs,snapshot_loader.rs,snapshot_loader_test.rs} |
| snapshot 后无间隙 watch、重放及 revision、progress/断流边界 | 3：snapshot_watch.rs / snapshot_watch_test.rs，同一 crate |
| 压缩全量恢复、custom reconciliation、失败退避、删除账本与幂等性 | 4：watch_reconcile.rs / watch_reconcile_test.rs；保留默认行为差异 |
| 初始化有限重试、success/retry 钩子、ForceLoad 合并/限流、WaitLoad 错误和取消 | 5：watch_control.rs / watch_control_test.rs |
| TTL Grant+Put、失败 revoke、安全处理不确定提交、到期删除 | 6：ttl_put.rs / ttl_put_test.rs |
| 具体消费方 JSON/proto 解码、完整 Config/PersistOptions、业务缓存与 gRPC 流装配 | 上层主题 3 对应领域适配/配置任务；当前仅固定消费契约，不迁移业务算法 |
| etcd 集群增删成员、HTTP、健康巡检、专用指标 | 上层网络/启动/运维主题；明确不属本子计划 |

所有生产测试应独立 *_test.rs 并显式注册；任务 2–6 使用唯一 kv::MetaStore，
不引入私有 runtime/全局集群 ID，不将 mock 调用次数或零测试作为验收。
本次清单不宣称整个 etcdutil、任何消费包或来源史诗已迁移完成。

### 本轮验证

- `rg -n '^(func |type |pub |    pub )' pkg/utils/etcdutil/etcdutil.go`：退出 0；
  补读未匹配的 const、完整多行签名与所有私有方法。
- 全仓 `NewLoopWatcher`、`EtcdKVPutWithTTL`、setter、ForceLoad/WaitLoad/StartWatchLoop
  反向搜索及逐项源码核对：覆盖上述全部生产构造点和测试入口。
- `git diff 5d5948052 -- pkg/utils/etcdutil/etcdutil.go`：退出 0、无输出。
- Markdown 相对链接及代码引用路径核验：36 个相对链接全部存在；未来文件已明确标拟建。
- `git diff --check`：退出 0；审查仅新增本证据文档及本编号任务收尾，不包含代码变更。
- Go/模块开始与结束路径及 SHA-256 检查：1101 个文件完全一致，相关 Git 状态为空。
  未运行 Go/Cargo 测试，未取得或声称新的跨语言运行证据。

# 控制面与MCS来源覆盖

任务002，2026-11-04。准入见[admission.md](admission.md)。
本清单固定责任，不认证尚未实现的行为。任务必须依次完成其直接依赖和真实差分。

机器可读完整清单：[task-002-coverage.json.gz](task-002-coverage.json.gz)。
每个manifest行保存源码路径、SHA-256、行数、责任任务和是否测试；records逐行保留
函数/类型声明、API、指标、错误、持久键引用、worker及failpoint的原文和责任任务。
同一源文件跨多个行为任务时保留共同责任，不能凭文件已出现就将所有行为标通过；
具体行为实施时必须在其task文档收窄到实际符号及运行场景。

Go基准 `461f6b8c41262b19a4c5d4acab26f49ae65ebbea`；协议 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。
固定150个源码文件、5416个带位置来源行；分类计数（可重叠）：`{'key': 1384, 'function': 2459, 'error': 596, 'worker': 351, 'type': 253, 'api': 203, 'fault': 351, 'metric': 193}`。
manifest canonical JSON SHA-256：`c64890bb063a0b262a486601df0701cd99e2b41d8e19fbbdfa81431cb79daf9b`。

完整保留所有源码文件哈希，故未匹配扫描表达式的const/var初始化和多行注册仍在
manifest的审查范围内；records是检索定位，不宣称正则是完整Go AST或独立业务数量。
具体常量/指标HELP、持久key格式须在行为任务读取整段源码并固定实际协议。

## 补齐邻接来源

原编号任务的“只读入口”没有显式列出34个范围内Go文件；已逐项补充责任，
包括kms.go、GC/replication/keyspace及MCS指标、token/RU/metering辅助逻辑、
watcher/额度集成测试和测试工具。只补此清单，不改只读plan或来源史诗。
这34项不是忽略项，也不因旧配置crate存在而自动完成。

| 邻接来源 | 责任任务 |
| --- | --- |
| [pkg/encryption/config.go](../../../pkg/encryption/config.go) | 9, 10, 11, 134 |
| [pkg/encryption/config_test.go](../../../pkg/encryption/config_test.go) | 9, 10, 11, 134 |
| [pkg/encryption/kms.go](../../../pkg/encryption/kms.go) | 11 |
| [pkg/gc/gc_state_warmup_scheduler_test.go](../../../pkg/gc/gc_state_warmup_scheduler_test.go) | 50, 51, 52 |
| [pkg/gc/metrics.go](../../../pkg/gc/metrics.go) | 53 |
| [pkg/gc/metrics_test.go](../../../pkg/gc/metrics_test.go) | 53 |
| [pkg/keyspace/constant/constant.go](../../../pkg/keyspace/constant/constant.go) | 25 |
| [pkg/keyspace/metrics.go](../../../pkg/keyspace/metrics.go) | 36, 37, 38, 39, 40, 41, 131 |
| [pkg/mcs/docs.go](../../../pkg/mcs/docs.go) | 1, 149 |
| [pkg/mcs/resourcemanager/server/keyspace_manager_test.go](../../../pkg/mcs/resourcemanager/server/keyspace_manager_test.go) | 93, 96, 97 |
| [pkg/mcs/resourcemanager/server/metering.go](../../../pkg/mcs/resourcemanager/server/metering.go) | 102, 103 |
| [pkg/mcs/resourcemanager/server/metering_test.go](../../../pkg/mcs/resourcemanager/server/metering_test.go) | 102, 103 |
| [pkg/mcs/resourcemanager/server/metrics.go](../../../pkg/mcs/resourcemanager/server/metrics.go) | 103 |
| [pkg/mcs/resourcemanager/server/metrics_test.go](../../../pkg/mcs/resourcemanager/server/metrics_test.go) | 103 |
| [pkg/mcs/resourcemanager/server/resource_group_test.go](../../../pkg/mcs/resourcemanager/server/resource_group_test.go) | 90, 91, 92, 95, 97, 99 |
| [pkg/mcs/resourcemanager/server/ru_version_policy_test.go](../../../pkg/mcs/resourcemanager/server/ru_version_policy_test.go) | 99 |
| [pkg/mcs/resourcemanager/server/testutil.go](../../../pkg/mcs/resourcemanager/server/testutil.go) | 104, 138, 146, 148 |
| [pkg/mcs/resourcemanager/server/utils.go](../../../pkg/mcs/resourcemanager/server/utils.go) | 89, 91, 99 |
| [pkg/mcs/resourcemanager/server/utils_test.go](../../../pkg/mcs/resourcemanager/server/utils_test.go) | 89, 91, 99 |
| [pkg/mcs/router/server/metrics.go](../../../pkg/mcs/router/server/metrics.go) | 88, 137 |
| [pkg/mcs/router/server/testutil.go](../../../pkg/mcs/router/server/testutil.go) | 88, 137, 145, 148 |
| [pkg/mcs/scheduling/server/metrics.go](../../../pkg/mcs/scheduling/server/metrics.go) | 125 |
| [pkg/mcs/scheduling/server/testutil.go](../../../pkg/mcs/scheduling/server/testutil.go) | 118, 124, 139, 147, 148 |
| [pkg/mcs/tso/server/config_test.go](../../../pkg/mcs/tso/server/config_test.go) | 77 |
| [pkg/mcs/tso/server/metrics.go](../../../pkg/mcs/tso/server/metrics.go) | 72, 78, 79, 82 |
| [pkg/mcs/tso/server/testutil.go](../../../pkg/mcs/tso/server/testutil.go) | 82, 136, 144, 148 |
| [pkg/mcs/utils/constant/constant.go](../../../pkg/mcs/utils/constant/constant.go) | 54, 55, 56 |
| [pkg/replication/metrics.go](../../../pkg/replication/metrics.go) | 4, 5, 6, 7, 8 |
| [tests/integrations/mcs/members/member_test.go](../../../tests/integrations/mcs/members/member_test.go) | 54, 55, 61, 148 |
| [tests/integrations/mcs/resourcemanager/resource_manager_test.go](../../../tests/integrations/mcs/resourcemanager/resource_manager_test.go) | 92, 93, 95, 100, 101, 146, 148 |
| [tests/integrations/mcs/resourcemanager/server_test.go](../../../tests/integrations/mcs/resourcemanager/server_test.go) | 104, 138, 142, 148 |
| [tests/integrations/mcs/resourcemanager/service_limit_test.go](../../../tests/integrations/mcs/resourcemanager/service_limit_test.go) | 96, 107, 146, 148 |
| [tests/integrations/mcs/resourcemanager/watcher_matrix_test.go](../../../tests/integrations/mcs/resourcemanager/watcher_matrix_test.go) | 98, 146, 148 |
| [tests/integrations/mcs/utils/testutil.go](../../../tests/integrations/mcs/utils/testutil.go) | 3, 148 |

## 行为责任与当前证据

| 任务 | 行为与契约 | 来源文件数 | 现有Rust入口（存在/拟建） | 证据 |
| --- | --- | ---: | --- | --- |
| 1 核对主题七准入证据 | 逐项核对主题7协议、存储、选主、运行时和调度公开API的当前证据；标明未提交并行变更与未完成依赖，建立实际crate映射，缺失前置证据不得放行代码任务。 | 1 | `pkg/proto/build.rs`（存在） | [准入完成](admission.md) |
| 2 固定来源覆盖清单 | 按生产包、公开符号、API、指标、持久化键、failpoint和Go测试列出主题8范围；每行映射本计划任务，识别已有实现与缺口，固定Go提交及协议摘要。 | 0 | `pkg/proto/build.rs`（存在） | 本清单；盘点完成 |
| 3 建立只读协议差分入口 | 复用已有Go二进制及只读fixtures，驱动同场景Go和Rust端点，保留状态码、响应和副作用；输入不匹配必须失败，缺少二进制不得跳过或输出通过。 | 1 | `pkg/proto/build.rs`（存在）；`tests/rust-compat/lib.rs`（存在）；`tests/rust-compat/oracle_runner.rs`（拟建） | [聚焦通过，待统一回归](task-003.md) |
| 4 恢复复制模式持久状态 | NewReplicationModeManager与UpdateConfig恢复模式和state ID，非法模式或持久化失败不发布新状态。 | 3 | `pkg/replication/lib.rs`（拟建）；`pkg/replication/mode_state.rs`（拟建） | [4聚焦+9相邻通过，待统一回归](task-004.md) |
| 5 实现同步转异步等待 | 按minimalUpVoters和store存活状态从sync转async_wait；多数派边界、可用副本不足与重复tick不错误推进。 | 3 | `pkg/replication/async_wait.rs`（拟建）；`pkg/replication/lib.rs`（拟建） | [4聚焦+4相邻通过，待统一回归](task-005.md) |
| 6 实现异步确认与超时 | async_wait等待store确认后转async，覆盖未确认、超时和重复报告，保持state ID与暂停split条件。 | 3 | `pkg/replication/async_confirm.rs`（拟建）；`pkg/replication/lib.rs`（拟建） | [4聚焦+4相邻通过，待统一回归](task-006.md) |
| 7 实现同步恢复进度 | async转sync_recover再回sync；恢复region扫描覆盖split与merge，进度和状态发布符合Go oracle。 | 3 | `pkg/replication/lib.rs`（拟建）；`pkg/replication/sync_recover.rs`（拟建） | [4聚焦+5相邻通过，待统一回归](task-007.md) |
| 8 接通复制状态运行周期 | 运行时启动单个复制模式worker，心跳报告驱动同一manager并暴露HTTP状态与对应指标；切主取消旧worker不再写状态。 | 3 | `pkg/replication/lib.rs`（拟建）；`pkg/replication/mode_worker.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在）；`pkg/server/lib.rs`（存在） | [2聚焦+23相邻通过，待统一回归](task-008.md) |
| 9 实现数据密钥密码操作 | KeyLength与数据密钥生成覆盖所有既有AES模式；使用Go已保存向量验证GCM密文与认证失败，随机密钥使用密码学随机源。 | 10 | `pkg/encryption/crypter.rs`（拟建）；`pkg/encryption/lib.rs`（存在） | [4聚焦+2相邻通过，待统一回归](task-009.md) |
| 10 读取文件主密钥 | 明文与文件主密钥加载及封装解封装匹配Go；长度错误、缺失文件与损坏密文明确报错且日志不含密钥。 | 10 | `pkg/encryption/lib.rs`（存在）；`pkg/encryption/master_key_file.rs`（拟建） | [4聚焦+11相邻通过，待统一回归](task-010.md) |
| 11 接入KMS主密钥 | 复用官方AWS Rust SDK凭据链完成数据密钥封装解封装；用官方测试替身验证错误映射、region与endpoint和日志脱敏，不访问真实付费资源。 | 11 | `pkg/encryption/kms.rs`（拟建）；`pkg/encryption/lib.rs`（存在） | [4聚焦+14相邻通过，待统一回归](task-011.md) |
| 12 持久化密钥字典 | 加载和保存key dictionary保持revision、当前key与历史key一致，损坏字典及提交失败不替换可读快照。 | 8 | `pkg/encryption/key_dictionary.rs`（拟建）；`pkg/encryption/lib.rs`（存在） | [6聚焦+8相邻通过，待统一回归](task-012.md) |
| 13 实现任期内密钥轮换 | 仅有效leader轮换密钥，处理碰撞重试与明文暴露标志；旧任期和事务失败不得发布新key。 | 8 | `pkg/encryption/key_rotation.rs`（拟建）；`pkg/encryption/lib.rs`（存在） | [5聚焦+14相邻通过，待统一回归](task-013.md) |
| 14 恢复密钥字典监听 | watch更新、断线重连和compaction重建后密钥快照收敛，关闭后不再更新。 | 8 | `pkg/encryption/key_watch.rs`（拟建）；`pkg/encryption/lib.rs`（存在） | [5聚焦+23相邻通过，待统一回归](task-014.md) |
| 15 接通Region边界加密 | region start/end key按既有方法与key ID加解密，保留输入对象、空边界和未知key错误；存储适配调用同一实现。 | 8 | `pkg/encryption/lib.rs`（存在）；`pkg/encryption/region_crypter.rs`（存在）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在）；`pkg/server/lib.rs`（存在） | [task-015](task-015.md)：9聚焦+34相邻通过；真实Go LevelDB双向CTR、heartbeat重启、历史key；待统一全量回归 |
| 16 验证恢复请求与阶段启动 | RemoveFailedStoresWithOptions拒绝非法或冲突请求，显式选项和自动识别模式产生一致初始阶段且暂停调度。 | 2 | `pkg/unsaferecovery/lib.rs`（存在）；`pkg/unsaferecovery/recovery_start.rs`（存在） | [task-016](task-016.md)：5聚焦+13相邻通过；真实Go/Rust准入、etcd故障、调度暂停；待统一全量回归 |
| 17 收集恢复心跳报告 | CollectReport与HandleStoreHeartbeat仅收取当前阶段报告，重复、迟到与缺失报告不提前进入下一阶段。 | 2 | `pkg/unsaferecovery/collect_report.rs`（存在）；`pkg/unsaferecovery/lib.rs`（存在） | [task-017](task-017.md)：4聚焦+13相邻通过；真实Go/Rust当前step、重复/缺失报告、统计旁路；待统一全量回归 |
| 18 构建恢复Region索引 | 按epoch与raft新鲜度构建重叠region索引，覆盖空范围、未初始化peer及merge重叠选择。 | 2 | `pkg/unsaferecovery/lib.rs`（存在）；`pkg/unsaferecovery/region_index.rs`（存在） | [task-018](task-018.md)：7聚焦+9相邻通过；实际Go计划匹配epoch范围及raft选择；待统一全量回归 |
| 19 生成ForceLeader计划 | 从报告选择可恢复leader，处理joint state和commit merge；相同输入产生等价计划与受影响region集合。 | 2 | `pkg/unsaferecovery/force_leader.rs`（存在）；`pkg/unsaferecovery/lib.rs`（存在） | [task-019](task-019.md)：6聚焦+25相邻通过；真实Go普通/commit-merge/joint计划；修正初始step；待统一全量回归 |
| 20 生成降级失败副本计划 | generateDemoteFailedVoterPlan仅降级失败投票副本，保留健康peer并正确处理learner。 | 2 | `pkg/unsaferecovery/demote_failed.rs`（存在）；`pkg/unsaferecovery/lib.rs`（存在） | [task-020](task-020.md)：6聚焦+21相邻通过；真实Go四种demote/learner/旧重叠规则；待统一全量回归 |
| 21 生成TiFlash清理计划 | generateTombstoneTiFlashLearnerPlan只清理目标learner，不误删TiKV可恢复副本。 | 2 | `pkg/unsaferecovery/lib.rs`（存在）；`pkg/unsaferecovery/tiflash_cleanup.rs`（存在） | [task-021](task-021.md)：6聚焦+25相邻通过；真实Go新鲜度清理与TiKV保留；待统一全量回归 |
| 22 生成空Region补洞计划 | generateCreateEmptyRegionPlan覆盖头尾和整段空洞，paranoid选项与TiKV候选筛选匹配Go，禁止覆盖健康数据范围。 | 2 | `pkg/unsaferecovery/empty_region.rs`；宿主真实ID接线 | [当前证据](task-022.md)，7聚焦+37相邻通过，待统一回归 |
| 23 分发并推进恢复计划 | dispatchPlan按阶段心跳确认推进，重复下发幂等，退出force-leader后恢复调度并提供稳定Show结果。 | 2 | `pkg/unsaferecovery/dispatch_plan.rs`；生产心跳及Show接线 | [当前证据](task-023.md)，5聚焦+46相邻通过，待统一回归 |
| 24 终止超时恢复任务 | AbortFailedStoresRemoval与checkTimeout在错误、无心跳及主动终止时停止下发并回收运行状态；接通现有管理API而不重写其通用层。 | 2 | `pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在）；`pkg/server/lib.rs`（存在）；`pkg/unsaferecovery/lib.rs`（拟建）；`pkg/unsaferecovery/recovery_timeout.rs`（拟建） | [当前证据](task-024.md)，5聚焦+54相邻通过，待统一回归 |
| 25 实现键空间标识与边界 | 校验ID和名称及保留空间，生成raw/txn边界和label rule，覆盖最大ID及不合法编码。 | 5 | `pkg/keyspace/keyspace_bounds.rs`；复用codec/labeler | [当前证据](task-025.md)，5聚焦+45相邻通过，待统一回归 |
| 26 实现键空间范围缓存 | Cache的Save/Delete/范围查找和跨键空间分割键保持有序、边界与删除可见性。 | 4 | `pkg/keyspace/keyspace_cache.rs`；真实共享缓存及分割算法 | [实现与当前证据](task-026.md)，65项Rust验证通过；同源Go四项逐例JUnit及14/12组断言重放通过，已解除差分阻塞，待统一工作区回归 |
| 27 分配元服务分组 | PickGroup与AssignToGroup在事务中更新计数，空间创建失败回滚分配且并发不会丢失计数。 | 2 | `pkg/keyspace/lib.rs`；`pkg/keyspace/meta_group_assign.rs` | [已完成](task-027.md)：5项Rust聚焦；同源Go直接四例、真实etcd冲突/取消及WAL/创建路径证据齐备 |
| 28 更新元服务分组端点 | UpdateGroupsSafely验证端点健康及已分配空间限制，失败不发布部分配置；AttachEndpoints不污染原始配置。 | 2 | `pkg/keyspace/lib.rs`；`pkg/keyspace/meta_group_update.rs` | [已完成](task-028.md)：7聚焦、22包级、Go健康/目录及7规范化逐例通过；宿主装配待131 |
| 29 管理时间戳分组目录 | Bootstrap及分组CRUD保持默认组和user-kind索引，分页与非法删除匹配Go。 | 3 | `pkg/keyspace/group_catalog.rs`；`pkg/storage/domain/keyspace_group_*.rs` | 已完成：[task-029](task-029.md)，Go 3例、Rust 2项真实etcd及相邻12项；默认组/CRUD/分页/重启索引 |
| 30 更新键空间分组归属 | UpdateKeyspaceGroup在源目标间原子迁移成员，条件不匹配与重试不重复计数。 | 3 | `pkg/keyspace/group_membership.rs` | 已完成：[task-030](task-030.md)，Go 3叶例、Rust 4项真实etcd；迁移/重复/保护/实际提交失败及WAL恢复 |
| 31 拆分时间戳分组 | SplitKeyspaceGroupByID及FinishSplit维护源目标状态和成员归属，拒绝并发冲突拆分。 | 3 | `pkg/keyspace/group_split.rs` | 已完成：[task-031](task-031.md)，Go 3例、Rust 5项；拆分/完成/并发/revision/提交故障及WAL恢复 |
| 32 合并时间戳分组 | MergeKeyspaceGroups和FinishMerge含默认组合并遵守容量、状态与重试语义。 | 3 | `pkg/keyspace/group_merge.rs`；`tests/rust-compat/group_merge_oracle_test.rs` | 已完成：[task-032](task-032.md)，Go 1例+在线默认组合并、Rust 5项真实etcd；容量/故障/WAL/物理字节 |
| 33 分配时间戳服务成员 | AllocNodes与SetNodes及priority更新基于去重节点列表，副本不足和无效候选不破坏现有成员。 | 3 | `pkg/keyspace/group_nodes.rs`；`tests/rust-compat/group_nodes_oracle_test.rs` | 已完成：[task-033](task-033.md)，Go 2例+在线注册目录/优先级、Rust 5项真实etcd；去重/事务故障/WAL |
| 34 恢复分组调和监听 | 节点与分组watch在compaction重载及任期变化后重新分配，旧任期取消后不提交结果。 | 3 | `pkg/keyspace/group_reconcile.rs`（拟建）；`pkg/keyspace/lib.rs`（拟建） | 待当前行为证据 |
| 35 自动拆分超大分组 | 巡检只拆分超过限制的分组并选择可用ID，ID耗尽与取消保持现状。 | 3 | `pkg/keyspace/group_patrol.rs`（拟建）；`pkg/keyspace/lib.rs`（拟建） | 待当前行为证据 |
| 36 完成键空间Region分割 | 创建时复用labeler和splitter等待边界到位；超时、取消和已存在边界遵循Go的可观察结果。 | 5 | `pkg/keyspace/keyspace_split.rs`（拟建）；`pkg/keyspace/lib.rs`（拟建） | 待当前行为证据 |
| 37 创建并查询键空间 | CreateKeyspace和CreateKeyspaceByID原子维护name与ID索引，重名冲突及LoadRange分页匹配Go。 | 5 | `pkg/keyspace/keyspace_create.rs`（拟建）；`pkg/keyspace/lib.rs`（拟建） | 待当前行为证据 |
| 38 更新键空间配置前置条件 | UpdateKeyspaceConfigWithPreconditions按CAS合并与删除配置，前置条件不满足或事务失败保持原状态。 | 5 | `pkg/keyspace/keyspace_config.rs`（拟建）；`pkg/keyspace/lib.rs`（拟建） | 待当前行为证据 |
| 39 迁移键空间生命周期 | UpdateKeyspaceState和ByID入口遵循状态转移限制，保护保留空间并按原行为失效GC缓存。 | 5 | `pkg/keyspace/keyspace_state.rs`（拟建）；`pkg/keyspace/lib.rs`（拟建） | 待当前行为证据 |
| 40 删除键空间元数据 | 删除仅作用于符合状态与前置条件的空间，原子清理name索引、分组计数及缓存，失败可重试。 | 5 | `pkg/keyspace/keyspace_remove.rs`（拟建）；`pkg/keyspace/lib.rs`（拟建） | 待当前行为证据 |
| 41 初始化保留键空间 | Bootstrap重复运行幂等，初始化默认及保留空间；事务失败不产生半成品索引。 | 5 | `pkg/keyspace/keyspace_bootstrap.rs`（拟建）；`pkg/keyspace/lib.rs`（拟建） | 待当前行为证据 |
| 42 批量移除分组键空间 | RemoveKeyspacesFromGroup按事务容量分批且每批CAS，部分失败重试不误删不合格空间。 | 3 | `pkg/keyspace/group_remove.rs`（拟建）；`pkg/keyspace/lib.rs`（拟建） | 待当前行为证据 |
| 43 读取键空间GC状态 | GetGCState从真实存储加载并按管理类型重定向，缓存miss与直接读错误保持一致。 | 11 | `pkg/gc/gc_read.rs`（拟建）；`pkg/gc/lib.rs`（拟建） | 待当前行为证据 |
| 44 推进事务安全点 | AdvanceTxnSafePoint在CAS中单调推进，过旧值与冲突不倒退。 | 11 | `pkg/gc/lib.rs`（拟建）；`pkg/gc/txn_safe_point.rs`（拟建） | 待当前行为证据 |
| 45 维护键空间GC屏障 | SetGCBarrier和DeleteGCBarrier保持TTL、最小安全点及服务标识语义，过期屏障不阻止推进。 | 11 | `pkg/gc/gc_barrier.rs`（拟建）；`pkg/gc/lib.rs`（拟建） | 待当前行为证据 |
| 46 维护全局GC屏障 | SetGlobalGCBarrier与删除跨空间检查最大事务安全点，非法全局屏障不部分生效。 | 11 | `pkg/gc/gc_global_barrier.rs`（拟建）；`pkg/gc/lib.rs`（拟建） | 待当前行为证据 |
| 47 推进GC安全点 | AdvanceGCSafePoint受事务安全点和全局及局部屏障约束；既有Compatible入口映射同一状态机，无新增旧格式迁移。 | 11 | `pkg/gc/gc_safe_point.rs`（拟建）；`pkg/gc/lib.rs`（拟建） | 待当前行为证据 |
| 48 快照加载启用键空间 | 固定revision分页扫描启用空间，页面边界与损坏meta处理匹配Go，不混合两个revision。 | 11 | `pkg/gc/enabled_snapshot.rs`（拟建）；`pkg/gc/lib.rs`（拟建） | 待当前行为证据 |
| 49 恢复启用键空间监听 | 从快照revision接续watch，删除、compaction和重连后snapshotAtLeast满足进度屏障。 | 11 | `pkg/gc/enabled_watch.rs`（拟建）；`pkg/gc/lib.rs`（拟建） | 待当前行为证据 |
| 50 批量预热GC缓存 | 预热按分页和批量限制读取状态，前台读取与预热并发不覆盖新状态。 | 12 | `pkg/gc/gc_warmup.rs`（拟建）；`pkg/gc/lib.rs`（拟建） | 待当前行为证据 |
| 51 重试GC预热失败 | 重试队列按期限调度失败scope，取消时停止重试，重复事件不造成忙循环或饥饿。 | 12 | `pkg/gc/gc_retry.rs`（拟建）；`pkg/gc/lib.rs`（拟建） | 待当前行为证据 |
| 52 隔离GC领导任期 | OnNodeBecomesLeader创建新generation并回收旧任务，旧加载结果不能回写新缓存。 | 12 | `pkg/gc/gc_generation.rs`（拟建）；`pkg/gc/lib.rs`（拟建） | 待当前行为证据 |
| 53 暴露GC状态与屏障指标 | GetAllKeyspacesGCStates与collector读取一致状态，删除空间和换代清理旧标签；接通现有GC管理入口。 | 13 | `pkg/gc/gc_metrics.rs`（拟建）；`pkg/gc/lib.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在）；`pkg/server/lib.rs`（存在） | 待当前行为证据 |
| 54 校验预期主节点候选 | IsValidPrimaryCandidate去重start时间并校验group成员，失效或非成员候选不能成为预期primary。 | 5 | `pkg/mcs/utils/lib.rs`（拟建）；`pkg/mcs/utils/primary_candidate.rs`（拟建） | 待当前行为证据 |
| 55 转移预期主节点标记 | TransferPrimary和ExpectedPrimaryCmp在lease与CAS保护下更新标记，旧primary不得覆盖新目标。 | 5 | `pkg/mcs/utils/lib.rs`（拟建）；`pkg/mcs/utils/primary_transfer.rs`（拟建） | 待当前行为证据 |
| 56 复用公共探针与指标处理 | StatusHandler与PromHandler使用宿主Registry与实际服务状态，多个实例不重复注册且不触碰全局日志或信号。 | 4 | `pkg/mcs/utils/lib.rs`（拟建）；`pkg/mcs/utils/service_probe.rs`（拟建） | 待当前行为证据 |
| 57 注册服务工厂与路由 | 同一宿主的gRPC与HTTP使用同一service实例，重复或冲突注册按Go行为报错，两个宿主不共享可变service对象。 | 2 | `pkg/mcs/registry/lib.rs`（拟建）；`pkg/mcs/registry/service_registry.rs`（拟建） | 待当前行为证据 |
| 58 编解码服务注册记录 | ServiceRegistryEntry字段与序列化匹配Go，未知服务名和损坏记录遵循Discover及GetMSMembers处理。 | 6 | `pkg/mcs/discovery/lib.rs`（拟建）；`pkg/mcs/discovery/registry_entry.rs`（拟建） | 待当前行为证据 |
| 59 发布服务租约 | Register以lease发布唯一记录，TTL续租失败重建前核实归属，避免两个进程抢写同一记录。 | 6 | `pkg/mcs/discovery/lib.rs`（拟建）；`pkg/mcs/discovery/service_register.rs`（拟建） | 待当前行为证据 |
| 60 安全撤销服务租约 | Deregister只撤销自己持有的lease；被抢占、重复关闭与延迟keepalive不删除新实例记录。 | 6 | `pkg/mcs/discovery/lib.rs`（拟建）；`pkg/mcs/discovery/service_deregister.rs`（拟建） | 待当前行为证据 |
| 61 验证服务发现收敛 | 真实MetaStore注册、过期及重连后Discover和GetMSMembers反映当前成员，保留未知服务错误而非空成功。 | 7 | `pkg/mcs/discovery/lib.rs`（拟建）；`pkg/mcs/discovery/service_discover.rs`（拟建） | 待当前行为证据 |
| 62 装配共享宿主监听器 | BaseServer以宿主Tokio handle与listener运行HTTP和gRPC，ready只在路由就绪后发布；绑定失败回收已创建资源。 | 2 | `pkg/mcs/server/base_listener.rs`（拟建）；`pkg/mcs/server/lib.rs`（拟建）；`pkg/runtime/node.rs`（存在） | 待当前行为证据 |
| 63 回收代理连接池 | GetDelegateClient复用正确TLS目标连接，关闭时取消进行中的请求并回收全部连接，跨宿主无串用。 | 2 | `pkg/mcs/server/delegate_connections.rs`（拟建）；`pkg/mcs/server/lib.rs`（拟建）；`pkg/runtime/node.rs`（存在） | 待当前行为证据 |
| 64 关闭服务与回调 | start callbacks只在就绪后调用，取消时等待所有后台任务及长流退出，重复shutdown安全且保留宿主runtime。 | 2 | `pkg/mcs/server/base_shutdown.rs`（拟建）；`pkg/mcs/server/lib.rs`（拟建）；`pkg/runtime/node.rs`（存在） | 待当前行为证据 |
| 65 暴露元存储范围读取 | MetaStorage.Get将range及revision映射到现有MetaStore，响应header和非serving错误匹配Go。 | 2 | `pkg/mcs/metastorage/server/lib.rs`（拟建）；`pkg/mcs/metastorage/server/metastorage_get.rs`（拟建） | 待当前行为证据 |
| 66 暴露元存储写入删除 | Put和Delete保持lease及prevKV与revision语义，失败不伪造成功header。 | 2 | `pkg/mcs/metastorage/server/lib.rs`（拟建）；`pkg/mcs/metastorage/server/metastorage_write.rs`（拟建） | 待当前行为证据 |
| 67 暴露元存储事件流 | Watch从指定revision推送事件与prevKV，DATA_COMPACTED含真实压缩revision并终止流，取消无泄漏。 | 2 | `pkg/mcs/metastorage/server/lib.rs`（拟建）；`pkg/mcs/metastorage/server/metastorage_watch.rs`（拟建） | 待当前行为证据 |
| 68 装配元存储服务 | 通过registry在现有宿主暴露同一MetaStore实例，独立宿主与嵌入宿主读写一致，不新建第二份存储状态。 | 2 | `pkg/mcs/metastorage/server/lib.rs`（拟建）；`pkg/mcs/metastorage/server/metastorage_host.rs`（拟建） | 待当前行为证据 |
| 69 转换时间戳数值 | ComposeTS和ParseTS边界及比较复用已有tso领域数值，协议转换留在适配层不重复时钟算法。 | 5 | `pkg/utils/tsoutil/lib.rs`（拟建）；`pkg/utils/tsoutil/timestamp_value.rs`（拟建） | 待当前行为证据 |
| 70 分发合并时间戳请求 | TSODispatcher批量汇聚count并正确分配响应，PD和TSO协议工厂共用分发逻辑，错误传播不泄漏pending请求。 | 5 | `pkg/utils/tsoutil/lib.rs`（拟建）；`pkg/utils/tsoutil/tso_dispatch.rs`（拟建） | 待当前行为证据 |
| 71 处理转发超时与停止 | WatchTSDeadline取消超时流，Stop完成所有排队请求；重连与重复停止不会双重完成请求。 | 5 | `pkg/utils/tsoutil/lib.rs`（拟建）；`pkg/utils/tsoutil/tso_deadline.rs`（拟建） | 待当前行为证据 |
| 72 装载分组时间戳分配器 | KeyspaceGroupManager根据元数据装配现有allocator，增删组收敛且默认组与保留空间映射正确。 | 4 | `pkg/tso/lib.rs`（存在）；`pkg/tso/tso_group_load.rs`（拟建） | 待当前行为证据 |
| 73 隔离分组时间戳领导权 | 每组使用现有选主与fencing，失去lease后不再发号，重新选主仍保持组内时间戳单调。 | 3 | `pkg/tso/lib.rs`（存在）；`pkg/tso/tso_group_leader.rs`（拟建） | 待当前行为证据 |
| 74 交接拆分分组时间戳 | 拆分目标同步源组时间戳后才serving，失败重试不能提前放行或生成倒退时间戳。 | 3 | `pkg/tso/lib.rs`（存在）；`pkg/tso/tso_group_split.rs`（拟建） | 待当前行为证据 |
| 75 交接合并分组时间戳 | 合并取各源组上界并完成元数据状态更新，部分源不可用时不错误宣告完成。 | 3 | `pkg/tso/lib.rs`（存在）；`pkg/tso/tso_group_merge.rs`（拟建） | 待当前行为证据 |
| 76 恢复分组分配器监听 | watch重连与compaction后重建实际分配器集合，关闭被删除组并释放其lease和worker。 | 3 | `pkg/tso/lib.rs`（存在）；`pkg/tso/tso_group_watch.rs`（拟建） | 待当前行为证据 |
| 77 校验时间戳服务配置 | 配置默认值与Parse及Adjust匹配Go，非法监听地址和更新周期在启动前报错。 | 9 | `pkg/mcs/tso/server/lib.rs`（拟建）；`pkg/mcs/tso/server/tso_service_config.rs`（拟建） | 待当前行为证据 |
| 78 实现时间戳服务流 | Tso校验cluster与keyspace group并调用既有allocator和dispatcher；双向流count、错误与取消和Go一致。 | 9 | `pkg/mcs/tso/server/lib.rs`（拟建）；`pkg/mcs/tso/server/tso_service_stream.rs`（拟建） | 待当前行为证据 |
| 79 查询分组与最小时间戳 | FindGroupByKeyspaceID和GetMinTS使用实际分组状态，未运行返回Unavailable且部分组未就绪不返回虚假下界。 | 9 | `pkg/mcs/tso/server/lib.rs`（拟建）；`pkg/mcs/tso/server/tso_service_query.rs`（拟建） | 待当前行为证据 |
| 80 暴露时间戳管理API | resetTS及分组成员查询调用既有领域服务，校验reset上限与转发目标，HTTP错误格式匹配Go。 | 8 | `pkg/mcs/tso/server/lib.rs`（拟建）；`pkg/mcs/tso/server/tso_service_admin.rs`（拟建） | 待当前行为证据 |
| 81 暴露时间戳主节点操作 | transfer及evict primary共用expected-primary规则，跨组转发不串组且重复请求不破坏leader。 | 8 | `pkg/mcs/tso/server/lib.rs`（拟建）；`pkg/mcs/tso/server/tso_service_primary.rs`（拟建） | 待当前行为证据 |
| 82 装配时间戳服务生命周期 | CreateServer对应Rust宿主入口注入同一分组manager，注册前完成ready，关闭释放lease与监听器并清理本服务指标。 | 10 | `pkg/mcs/tso/server/lib.rs`（拟建）；`pkg/mcs/tso/server/tso_service_host.rs`（拟建） | 待当前行为证据 |
| 83 同步路由Store元数据 | 初始化Store快照后接续watch，删除、断线和compaction重建保证BasicCluster最终一致。 | 2 | `pkg/mcs/router/server/meta/lib.rs`（拟建）；`pkg/mcs/router/server/meta/router_store_watch.rs`（拟建） | 待当前行为证据 |
| 84 校验路由服务配置 | Parse与adjust保留监听及广播地址和TLS默认值，非法backend与Clone隔离符合Go。 | 1 | `pkg/mcs/router/server/config/lib.rs`（拟建）；`pkg/mcs/router/server/config/router_config.rs`（拟建） | 待当前行为证据 |
| 85 同步路由Region状态 | RegionSyncer复用已有syncer按索引同步Region，切换PD leader及重复事件不回退范围树。 | 6 | `pkg/mcs/router/server/lib.rs`（拟建）；`pkg/mcs/router/server/router_region_sync.rs`（拟建） | 待当前行为证据 |
| 86 暴露路由单项查询 | GetRegion及ByID、PrevRegion、QueryRegion和Store查询映射同一缓存，空洞和不存在对象返回正确header。 | 6 | `pkg/mcs/router/server/lib.rs`（拟建）；`pkg/mcs/router/server/router_query.rs`（拟建） | 待当前行为证据 |
| 87 暴露路由范围扫描 | ScanRegions与BatchScanRegions保持范围边界、limit和连续性校验，跨split和空范围匹配Go。 | 6 | `pkg/mcs/router/server/lib.rs`（拟建）；`pkg/mcs/router/server/router_scan.rs`（拟建） | 待当前行为证据 |
| 88 装配路由服务生命周期 | 只有Store与Region初始化完成后才ready；停服撤销注册并停止sync，HTTP探针及本服务指标反映真实状态。 | 8 | `pkg/mcs/router/server/lib.rs`（拟建）；`pkg/mcs/router/server/router_host.rs`（拟建） | 待当前行为证据 |
| 89 校验资源服务启动配置 | 扩展现有RU配置以覆盖NewConfig、Parse和Adjust启动字段，验证监听、backend和TLS及controller配置默认值，不重复已有配置类型。 | 4 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_start_config.rs`（拟建） | 待当前行为证据 |
| 90 实现令牌桶补充与借贷 | GroupTokenBucket随时间补充、负债和burst设置变化保持Go结果，覆盖零速率及非有限输入。 | 14 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_bucket.rs`（拟建） | 待当前行为证据 |
| 91 实现客户端令牌槽分配 | 按clientUniqueID分配slot并回收过期slot，借贷平衡和并发请求不重复发放令牌。 | 16 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_slot.rs`（拟建） | 待当前行为证据 |
| 92 维护资源组配置 | ResourceGroup.ApplySettings与PatchSettings验证RU模式及priority，Clone隔离可变状态，设置变更不重置运行余额。 | 15 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_group.rs`（拟建） | 待当前行为证据 |
| 93 隔离键空间资源组 | 按keyspace管理默认组、保留组和普通组，CRUD持久化与重启恢复不跨空间泄漏。 | 15 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_keyspace.rs`（拟建） | 待当前行为证据 |
| 94 控制资源组写角色 | LegacyAll、PDMetaOnly与RMTokenOnly严格区分元数据和运行状态写权限，禁止路径保持明确错误。 | 13 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_write_role.rs`（拟建） | 待当前行为证据 |
| 95 持久化资源组运行余额 | persistLoop只在可写角色和有效任期保存状态，重启恢复余额，失败与取消不覆盖新值。 | 15 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_state.rs`（拟建） | 待当前行为证据 |
| 96 限制键空间总服务额度 | ServiceLimiter按时间补充并在多个组间限制总额度，零与burst及更新后上限一致。 | 15 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_service_limit.rs`（拟建） | 待当前行为证据 |
| 97 分配优先级组填充速率 | 按真实RU需求及priority分配基础和burst额度，空需求与过期tracker清理不造成超发。 | 15 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_conciliate.rs`（拟建） | 待当前行为证据 |
| 98 监听资源管理元数据 | metadata watcher按快照revision恢复resource group、service limit和配置，删除与compaction后不保留幽灵组。 | 14 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_watch.rs`（拟建） | 待当前行为证据 |
| 99 应用RU版本策略 | 默认与keyspace override选择版本，非法版本不生效，运行更新不污染其他空间。 | 17 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_ru_policy.rs`（拟建） | 待当前行为证据 |
| 100 暴露资源组gRPC管理 | Get/List/Add/Modify/Delete映射同一manager并遵守写角色，非serving与缺失资源返回Go对应状态。 | 14 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_grpc_metadata.rs`（拟建） | 待当前行为证据 |
| 101 暴露令牌申请流 | AcquireTokenBuckets处理多组申请及消费报告，流关闭与部分组错误不会泄漏额度或污染其他空间。 | 14 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_grpc_token.rs`（拟建） | 待当前行为证据 |
| 102 聚合资源消费计量 | RU collector对OLTP、OLAP及跨AZ分类聚合与Go一致，删除组清理数据；复用已有metering writer契约不扩展主题9运维范围。 | 15 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_metering.rs`（拟建） | 待当前行为证据 |
| 103 导出资源消费指标 | 消费计数与最大速率按既有label和刷新窗口导出，删除组与服务关闭清理宿主Registry。 | 17 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_metrics.rs`（拟建） | 待当前行为证据 |
| 104 装配资源管理生命周期 | 扩展现有配置crate而非重建；初始化元数据与选主后ready，切主停止旧persistLoop，shutdown回收worker与lease。 | 15 | `pkg/mcs/resourcemanager/server/lib.rs`（存在）；`pkg/mcs/resourcemanager/server/rm_host.rs`（拟建） | 待当前行为证据 |
| 105 暴露资源组HTTP管理 | ConfigService解析keyspace ID的JSON类型、null和冲突字段，CRUD保持HTTP状态与响应并复用同一manager。 | 2 | `pkg/mcs/resourcemanager/metadataapi/lib.rs`（拟建）；`pkg/mcs/resourcemanager/metadataapi/rm_http_group.rs`（拟建） | 待当前行为证据 |
| 106 暴露资源控制配置HTTP | 控制器配置单项更新和查询遵守类型校验，失败不部分修改运行配置。 | 2 | `pkg/mcs/resourcemanager/metadataapi/lib.rs`（拟建）；`pkg/mcs/resourcemanager/metadataapi/rm_http_config.rs`（拟建） | 待当前行为证据 |
| 107 暴露键空间额度HTTP | 按name或ID查询及设置service limit，缺失空间与非法额度匹配Go错误映射。 | 3 | `pkg/mcs/resourcemanager/metadataapi/lib.rs`（拟建）；`pkg/mcs/resourcemanager/metadataapi/rm_http_limit.rs`（拟建） | 待当前行为证据 |
| 108 装配资源服务HTTP路由 | 挂载已完成metadataapi路由并暴露config、health和日志管理，路径和中间件错误响应匹配Go。 | 2 | `pkg/mcs/resourcemanager/server/apis/v1/lib.rs`（拟建）；`pkg/mcs/resourcemanager/server/apis/v1/rm_api_route.rs`（拟建） | 待当前行为证据 |
| 109 管理资源主节点与RU策略 | transferPrimary与setKeyspaceRUVersion复用现有领域规则，非法目标或版本不修改状态。 | 2 | `pkg/mcs/resourcemanager/server/apis/v1/lib.rs`（拟建）；`pkg/mcs/resourcemanager/server/apis/v1/rm_api_primary.rs`（拟建） | 待当前行为证据 |
| 110 路由资源元数据写请求 | 按部署与write role决定PD本地处理或重定向RM，fallback不绕过写权限，保留方法body和错误响应。 | 4 | `pkg/mcs/resourcemanager/redirector/lib.rs`（拟建）；`pkg/mcs/resourcemanager/redirector/rm_redirect.rs`（拟建） | 待当前行为证据 |
| 111 构造调度服务持久配置 | 复用既有schedule配置领域类型提供PersistConfig，默认值和配置读写保持Go语义，不复制配置状态。 | 4 | `pkg/mcs/scheduling/server/config/lib.rs`（拟建）；`pkg/mcs/scheduling/server/config/scheduling_config.rs`（拟建） | 待当前行为证据 |
| 112 监听调度配置及TTL | 配置watch与TTL过期触发scheduler notifier，重连快照恢复完整配置且Close解除通知。 | 4 | `pkg/mcs/scheduling/server/config/lib.rs`（拟建）；`pkg/mcs/scheduling/server/config/scheduling_config_watch.rs`（拟建） | 待当前行为证据 |
| 113 监听调度Store状态 | Store快照和watch复用BasicCluster，tombstone回调只影响对应store，compaction恢复和取消不保留过期store。 | 2 | `pkg/mcs/scheduling/server/meta/lib.rs`（拟建）；`pkg/mcs/scheduling/server/meta/scheduling_store_watch.rs`（拟建） | 待当前行为证据 |
| 114 加载调度规则快照 | 固定revision分页读取rule key和value并调和placement manager，边界空页和并发删除不遗漏规则。 | 3 | `pkg/mcs/scheduling/server/rule/lib.rs`（拟建）；`pkg/mcs/scheduling/server/rule/scheduling_rule_snapshot.rs`（拟建） | 待当前行为证据 |
| 115 监听Placement与Label规则 | 从快照接续规则和region label事件，删除和compaction后manager收敛且不产生重复rule。 | 3 | `pkg/mcs/scheduling/server/rule/lib.rs`（拟建）；`pkg/mcs/scheduling/server/rule/scheduling_rule_watch.rs`（拟建） | 待当前行为证据 |
| 116 监听亲和分组与标签 | group与affinity label监听更新既有affinity manager，初始快照及compaction重建不产生悬挂关联。 | 2 | `pkg/mcs/scheduling/server/affinity/lib.rs`（拟建）；`pkg/mcs/scheduling/server/affinity/scheduling_affinity_watch.rs`（拟建） | 待当前行为证据 |
| 117 监听调度键空间元数据 | keyspace meta解析与增删更新既有Cache，错误key不误删，重连和compaction恢复启用空间边界。 | 2 | `pkg/mcs/scheduling/server/keyspace_meta/lib.rs`（拟建）；`pkg/mcs/scheduling/server/keyspace_meta/scheduling_keyspace_watch.rs`（拟建） | 待当前行为证据 |
| 118 装配调度集群领域依赖 | NewCluster使用现有coordinator、statistics、operator和watcher，不复制调度内核；prepare门槛未满足不运行调度。 | 7 | `pkg/mcs/scheduling/server/lib.rs`（拟建）；`pkg/mcs/scheduling/server/scheduling_cluster.rs`（拟建） | 待当前行为证据 |
| 119 处理调度Region心跳 | RegionHeartbeat流复用现有心跳内核和hbstream，乱序与split更新正确且operator响应返回原流。 | 6 | `pkg/mcs/scheduling/server/lib.rs`（拟建）；`pkg/mcs/scheduling/server/scheduling_region_hb.rs`（拟建） | 待当前行为证据 |
| 120 处理调度Store与Bucket心跳 | StoreHeartbeat与RegionBuckets路由已验证统计入口，过期报告不覆盖新状态，非serving返回Unavailable。 | 6 | `pkg/mcs/scheduling/server/lib.rs`（拟建）；`pkg/mcs/scheduling/server/scheduling_store_hb.rs`（拟建） | 待当前行为证据 |
| 121 代理调度ID与批量切分 | AllocID及AskBatchSplit正确转发PD leader，换主重试不复用ID或返回部分成功。 | 6 | `pkg/mcs/scheduling/server/lib.rs`（拟建）；`pkg/mcs/scheduling/server/scheduling_split.rs`（拟建） | 待当前行为证据 |
| 122 暴露调度Split与Scatter | SplitRegions及ScatterRegions使用既有领域服务，取消、部分region失败与GetOperator观察结果一致。 | 6 | `pkg/mcs/scheduling/server/lib.rs`（拟建）；`pkg/mcs/scheduling/server/scheduling_operations.rs`（拟建） | 待当前行为证据 |
| 123 切换调度主节点 | campaignPrimary在元数据就绪后启动后台调度，失去lease停止operator输出和写入，换主重建仍复用领域状态。 | 6 | `pkg/mcs/scheduling/server/lib.rs`（拟建）；`pkg/mcs/scheduling/server/scheduling_primary.rs`（拟建） | 待当前行为证据 |
| 124 回收调度后台任务 | stopCluster等待coordinator、watcher及心跳任务退出，重复关闭与启动失败清理不影响宿主runtime。 | 7 | `pkg/mcs/scheduling/server/lib.rs`（拟建）；`pkg/mcs/scheduling/server/scheduling_shutdown.rs`（拟建） | 待当前行为证据 |
| 125 导出调度服务指标 | 服务指标与store标签沿用Go定义，tombstone及切主删除旧值，Registry由宿主注入。 | 7 | `pkg/mcs/scheduling/server/lib.rs`（拟建）；`pkg/mcs/scheduling/server/scheduling_metrics.rs`（拟建） | 待当前行为证据 |
| 126 暴露调度器与Checker管理 | 复用既有handler提供scheduler配置与诊断、pause及checker控制，路径和错误与MCS Go API一致。 | 3 | `pkg/mcs/scheduling/server/apis/v1/lib.rs`（拟建）；`pkg/mcs/scheduling/server/apis/v1/scheduling_api_scheduler.rs`（拟建） | 待当前行为证据 |
| 127 暴露Operator管理 | 创建删除及查询operator和records使用同一coordinator，批量失败与不存在行为匹配Go。 | 3 | `pkg/mcs/scheduling/server/apis/v1/lib.rs`（拟建）；`pkg/mcs/scheduling/server/apis/v1/scheduling_api_operator.rs`（拟建） | 待当前行为证据 |
| 128 暴露调度统计查询 | hotspot与store及region查询复用主题7适配器，MCS路由和字段匹配Go，避免另建统计状态。 | 3 | `pkg/mcs/scheduling/server/apis/v1/lib.rs`（拟建）；`pkg/mcs/scheduling/server/apis/v1/scheduling_api_stats.rs`（拟建） | 待当前行为证据 |
| 129 暴露规则与亲和查询 | placement、label及affinity只读HTTP复用领域manager，路径参数和缺失对象错误保持Go行为。 | 3 | `pkg/mcs/scheduling/server/apis/v1/lib.rs`（拟建）；`pkg/mcs/scheduling/server/apis/v1/scheduling_api_rules.rs`（拟建） | 待当前行为证据 |
| 130 暴露调度管理动作 | cache清理、加速、scatter和split映射已有领域handler，health、config、日志和primary路由复用公共处理器。 | 3 | `pkg/mcs/scheduling/server/apis/v1/lib.rs`（拟建）；`pkg/mcs/scheduling/server/apis/v1/scheduling_api_admin.rs`（拟建） | 待当前行为证据 |
| 131 接通键空间领域服务 | PD宿主注入同一keyspace及分组manager，Keyspace gRPC与管理HTTP操作同一状态并验证启动恢复。 | 6 | `pkg/runtime/control_keyspace.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在）；`pkg/server/lib.rs`（存在） | 待当前行为证据 |
| 132 接通GC领域服务 | PD宿主为GC服务提供任期与取消令牌，GC管理入口复用同一manager，换主不发布旧generation状态。 | 5 | `pkg/runtime/control_gc.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在）；`pkg/server/lib.rs`（存在） | 待当前行为证据 |
| 133 接通复制模式领域服务 | PD宿主向既有心跳路径注入同一replication manager，管理API与心跳报告观察一致状态。 | 5 | `pkg/runtime/control_replication.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在）；`pkg/server/lib.rs`（存在） | 待当前行为证据 |
| 134 接通加密领域服务 | PD宿主启动同一key manager并向Region存储注入加密适配，重启可解密且关闭结束轮换watch。 | 7 | `pkg/runtime/control_encryption.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在）；`pkg/server/lib.rs`（存在） | 待当前行为证据 |
| 135 接通恢复控制器领域服务 | PD宿主连接同一recovery controller到管理API和Store心跳，恢复阶段切换与调度暂停可观察一致。 | 5 | `pkg/runtime/control_recovery.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在）；`pkg/server/lib.rs`（存在） | 待当前行为证据 |
| 136 装配嵌入时间戳服务 | 宿主注入Handle、listener、取消令牌及Registry启动TSO服务，两个实例隔离且退出不停止宿主。 | 6 | `pkg/runtime/embed_tso.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在） | 待当前行为证据 |
| 137 装配嵌入路由服务 | 宿主共享runtime启动Router并通过真实查询确认缓存就绪，关闭回收监听器而保留宿主其他角色。 | 7 | `pkg/runtime/embed_router.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在） | 待当前行为证据 |
| 138 装配嵌入资源管理服务 | 宿主共享runtime启动RM并完成实际令牌请求，关闭只释放本实例任务与lease。 | 7 | `pkg/runtime/embed_rm.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在） | 待当前行为证据 |
| 139 装配嵌入调度服务 | 宿主共享runtime启动Scheduling处理真实心跳，shutdown后没有延迟operator输出。 | 6 | `pkg/runtime/embed_scheduling.rs`（拟建）；`pkg/runtime/lib.rs`（存在）；`pkg/runtime/node.rs`（存在） | 待当前行为证据 |
| 140 接通独立时间戳入口 | 在现有命令入口按Go已有服务选择启动相同TSO宿主，配置错误退出码与信号关闭可观察；不新增猜测性开关。 | 2 | `cmd/pd-server/bin_main.rs`（存在）；`cmd/pd-server/lib.rs`（存在）；`cmd/pd-server/main.rs`（存在）；`cmd/pd-server/standalone_tso.rs`（拟建） | 待当前行为证据 |
| 141 接通独立路由入口 | 独立Router进程使用与嵌入相同builder，真实查询通过且SIGTERM后端口与服务注册均释放。 | 2 | `cmd/pd-server/bin_main.rs`（存在）；`cmd/pd-server/lib.rs`（存在）；`cmd/pd-server/main.rs`（存在）；`cmd/pd-server/standalone_router.rs`（拟建） | 待当前行为证据 |
| 142 接通独立资源管理入口 | 独立RM进程使用同一builder与写角色规则，真实令牌请求通过且信号关闭不残留状态写任务。 | 3 | `cmd/pd-server/bin_main.rs`（存在）；`cmd/pd-server/lib.rs`（存在）；`cmd/pd-server/main.rs`（存在）；`cmd/pd-server/standalone_rm.rs`（拟建） | 待当前行为证据 |
| 143 接通独立调度入口 | 独立Scheduling进程使用同一builder，真实心跳可生成operator响应，启动失败与信号退出清理资源。 | 2 | `cmd/pd-server/bin_main.rs`（存在）；`cmd/pd-server/lib.rs`（存在）；`cmd/pd-server/main.rs`（存在）；`cmd/pd-server/standalone_scheduling.rs`（拟建） | 待当前行为证据 |
| 144 回归分组拆合跨服务链 | PD keyspace分组操作经MetaStorage与TSO watch传播，拆合期间发号单调且重新查询归属一致。 | 2 | `tests/rust-compat/accept_group.rs`（拟建）；`tests/rust-compat/lib.rs`（存在） | 待当前行为证据 |
| 145 回归路由换主收敛 | PD换主和Region同步断线后Router查询收敛，compaction重建不保留已删除Store。 | 2 | `tests/rust-compat/accept_router.rs`（拟建）；`tests/rust-compat/lib.rs`（存在） | 待当前行为证据 |
| 146 回归资源写角色链 | PD元数据写经RM watcher生效，RM发令牌仅写运行状态，断线与compaction后无权限绕过或额度重复发放。 | 5 | `tests/rust-compat/accept_rm.rs`（拟建）；`tests/rust-compat/lib.rs`（存在） | 待当前行为证据 |
| 147 回归调度监听恢复链 | 配置、规则及空间更新经watch驱动真实operator选择，切主及compaction恢复后旧任期不再调度。 | 2 | `tests/rust-compat/accept_scheduling.rs`（拟建）；`tests/rust-compat/lib.rs`（存在） | 待当前行为证据 |
| 148 回归双部署模式协议 | 同一请求集合覆盖MetaStorage及四个独立MCS的独立和嵌入模式，响应与副作用差分一致，包含TLS认证拒绝及取消。 | 11 | `tests/rust-compat/accept_modes.rs`（拟建）；`tests/rust-compat/lib.rs`（存在） | 待当前行为证据 |
| 149 审计来源覆盖与工作区 | 逐行关闭来源覆盖清单，检查全部包和服务的成功错误故障证据及无领域副本，再运行工作区回归；缺失任何来源能力不宣告主题完成。 | 2 | `tests/rust-compat/accept_workspace.rs`（拟建）；`tests/rust-compat/lib.rs`（存在） | 待当前行为证据 |

## 复核

生成时检查149个任务编号连续且唯一、manifest路径存在且唯一、每行责任任务有效、
147个主题范围Go文件全部进入manifest，所有引用的额外Go oracle文件也一并固定。
34个补充来源逐项复核到相应行为任务；Go源码/模块1101项首尾SHA与状态一致。
文档链接存在性和git diff --check退出0。无Cargo调用，新增行为测试0。
任务003的只读差分接线及聚焦验证见[task-003.md](task-003.md)；完整回归留待统一执行。其余“拟建”仍不表示接口可用。

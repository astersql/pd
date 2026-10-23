# 存储键值事务契约盘点（任务 1）

日期：2026-10-23（Asia/Shanghai）。状态：已完成文档盘点。
审计 HEAD：`feff860d03e07f97846a957edf0f263613312d98`；规划基准：
`5d5948052a9ee9e3fa7a09145fe2916a2912c223`。四个指定 Go 来源相对规划基准无差异。
本轮只读取源码、测试断言及装配，不运行 Go/Cargo，不生成 oracle，不改变运行行为。
以下“事实”为当前源码结构，“推导”为函数体可观察行为，“既有断言”不等于本轮运行结果。

## 准入与来源

批次 8 依赖批次 7。前序提交 `feff860d0` 同时包含生产模块、独立测试、Go oracle
及[键路径证据](theme3-存储键路径.md)任务 7 节；其原任务文件已在该提交删除。
已复核 `pkg/utils/keypath/lib.rs` 的生产/测试注册与
`pkg/utils/keypath/registry_marker_paths{,_test}.rs` 实物。前序记录为 186/186 Go/Rust
字节相等、聚焦 2 passed、workspace 459 passed / 0 failed / 1 ignored，检查均退出 0；
这是前序提交的历史证据，本轮未重跑。准入满足。

已读根 `AGENTS.md`、`.github/copilot-instructions.md`、[ADR-001](architecture.md)、
[依赖基线](dependencies.md)、[移植指南](porting-guide.md)及本任务只读计划和来源主题。
扫描仓库（排除 target/.git）只发现根 AGENTS.md，无更深覆盖指令。
本任务不修改来源主题、plan.md、Go/Rust 生产、测试、manifest 或 lock。

四个指定来源全篇读取（包括多行 `RunInTxnWithConditions` 签名），固定 SHA-256：

| 文件（仓库相对） | SHA-256 |
| --- | --- |
| `pkg/storage/kv/kv.go` | `963c1139aa64bb0ed3f81483c043c75626addc1ca86416a8fef3ff3a7ea6117c` |
| `pkg/storage/kv/etcd_kv.go` | `164444b80b892ca9ed7a8d9b174de7644c72ad54de59043d0f2d9affa41382a8` |
| `pkg/storage/kv/mem_kv.go` | `2776eb0f1526a71b7ed7fa881f71a7cd24ec877e5d6e53ea78b9e00a40edda76` |
| `pkg/storage/kv/levedb_kv.go` | `e6e45a2eaeb7ee0512c165a81ef2ecf70635faf92a7dcac6ce225a57523d7040` |

相邻 Go 文件为 `pkg/storage/kv/kv_test.go` 与 `metrics.go`；辅助依据为
`pkg/utils/etcdutil/etcdutil.go::{EtcdKVGet,EtcdKVGetWithContext}`、
`pkg/storage/endpoint/endpoint.go` 与各 backend 工厂。KV 包当前没有 Rust 文件或 Cargo.toml。

## 公开契约与符号覆盖

后续编号仅表示责任归属，不表示实现已完成：2 唯一 MetaStore 下沉，3 有界范围，
4 单键 CRUD，5 回调事务，6 revision 保护读，7 原子比较，8 分支写入，9 单键结果，
10 范围结果，11 取消/错误，12 watch/lease/fencing 回归。

| 来源 | 全部公开声明/方法与字段 | 归属 |
| --- | --- | --- |
| `kv.go:24–46` | `RawTxnCmpType`：`RawTxnCmpEqual=0,NotEqual=1,Less=2,Greater=3,Exists=4,NotExists=5`（后五项同带 RawTxnCmp 前缀）；`RawTxnOpType`：`RawTxnOpPut=0,Delete=1,Get=2,GetRange=3`（同带 RawTxnOp 前缀） | 7；8–10 |
| `kv.go:49–87` | `RawTxnCondition{Key,CmpType,Value}`；`RawTxnOp{Key,OpType,Value,EndKey,Limit}`；`KeyValuePair{Key,Value}`；`RawTxnResponseItem{KeyValuePairs}`；`RawTxnResponse{Succeeded,Responses}` | 7–10 |
| `kv.go:97–113` | `RawTxn::{If,Then,Else,Commit}`；`RawTxnCapable::CreateRawTxn`；`RawTxnWithContextCapable::CreateRawTxnWithContext` | 7–11 |
| `kv.go:116–137` | `BaseReadWrite::{Save,Remove,Load,LoadRange}`；`Txn` 嵌入 BaseReadWrite；`RevisionTxn` 嵌入 Txn，增加 `LoadWithRevision` | 3–6 |
| `kv.go:139–179` | `ConditionalTxnRunner::RunInTxnWithConditions(ctx,[]clientv3.Cmp,f)`；`Base` 嵌入 BaseReadWrite，增加 `RunInTxn(ctx,f)` | 5、6；etcd comparison 去协议化归 7，上层 fencing 装配归 election/member/endpoint 计划 |
| `etcd_kv.go:53–143` | `NewEtcdKVBase`；返回私有类型的公开方法 `Load,LoadRange,Save,Remove,CreateRawTxn,CreateRawTxnWithContext` | 2–4、7–11 |
| `etcd_kv.go:146–208` | `SlowLogTxn` 嵌入 `clientv3.Txn`；`NewSlowLogTxn,NewSlowLogTxnWithContext`；`If,Then,Commit`；Else 等未覆写方法由嵌入类型提供 | 请求生命周期归 11；Go 专用 client 包装替代归上层 etcdutil/election/member，遥测归上层服务/可观测性主题 |
| `etcd_kv.go:222–334` | 私有 etcdKVBase 的 `RunInTxn,RunInTxnWithConditions`；私有 etcdTxn 的 `Save,Remove,Load,LoadWithRevision,LoadRange` | 3、5、6、11 |
| `etcd_kv.go:344–474` | 私有 rawTxnWrapper 的 `If,Then,Else,Commit` | 7–11 |
| `mem_kv.go:34–200` | `NewMemoryKV`；私有 memoryKV 的 `Load,LoadRange,Save,Remove,RunInTxn`；memoryKVItem 的 `Less`；memTxn 的 `Save,Remove,Load,LoadRange` | 2–5；Less 为内部排序；后续共用契约以 Go etcd 为目标，保留差异证据 |
| `levedb_kv.go:30–152` | `LevelDBKV` 嵌入 `*leveldb.DB`；`NewLevelDBKV`；`Load,LoadRange,Save,Remove,RunInTxn`；私有 levelDBTxn 的 `Save,Remove,Load,LoadRange` | 3–5 行为参考；DB 提升方法及本地生产持久化归上层 storage/durability，不移植引擎 |

私有实现也已全篇核对：etcdTxn 的 `loadValue,commit`、`convertOps`、三个后端
commit；memoryKV/tree、memoryKVItem、memTxn、op/opType/tPut/tDelete；levelDBTxn/batch；
etcdKVBase、rawTxnWrapper 及其缓存 thenOps/elseOps；请求常量 requestTimeout=10s、
slowRequestTime=1s 和四个缓存 metrics handle。没有仅靠声明搜索代替函数体核对。

## key/value 与后端差异（源码推导）

键和值为 Go string（可含任意字节），KV 不解析业务 JSON/protobuf、不拼业务键路径；
Rust 现有 MetaStore 用 Vec<u8>，业务编码与 schema 属 endpoint/keypath 计划。
不能为了 UTF-8 转换丢失字节，也不能将缺失键与已存在空值混为同一原子比较对象。

| 行为与依据 | Go 实现事实/推导 | 现有 Rust 与缺口归属 |
| --- | --- | --- |
| Base Load/Save/Remove | 三后端 Load 缺失均返回 `"",nil`，与空值无法由 Base Load 区分；Save 覆盖，删除缺失无错误。etcd Load 多于 1 项为 ErrEtcdKVGetResponse；单键 CRUD 未附 lease | MetaStore 目前只有 prefix read 和 transact；4 建立准确点读/CRUD，不能 prefix 误命中相邻键 |
| LoadRange 正常区间 | memory BTree AscendRange，LevelDB util.Range，etcd WithRange；目标 `[key,endKey)`、升序、正 limit 截断、0 不限；返回 keys/values 对齐 | Read 只有 prefix/start，无任意 end；3 扩展唯一 Read，不先扫描再本地截断 |
| 特殊上界 | 只有 etcd 在 endKey 恰为 `"\x00"` 时调用 WithPrefix（空 key 为全表）。memory/LevelDB 不做 prefix 转换；Go testRange 主动跳过非 etcd 的该场景 | 3、10 保留 etcd 目标；不能将任意带 NUL 的上界当 sentinel（`txn-k2\x00` 是普通上界） |
| 非法/边界参数 | memory/LevelDB `make(...,0,limit)` 负 limit 可 panic；etcd 转交 WithLimit。空/反向区间、空 end 与 all-FF prefix 需按后端分别固定；不能由注释宣布等价 | 3、10、11 的真实 oracle 边界，不复制 Go panic 作为 Rust API；本轮无边界运行证据 |
| callback 成功/失败 | 三者先执行 f；f 错误原样返回且不 commit。Save/Remove 缓存，Load/LoadRange 直接读后端，看不到本事务未提交写；不是 repeatable-read 快照 | 无 callback API；5，回调错误与提交错误须可区分；不能提前写后再撤销 |
| callback 原子性 | memory commit 一次持树锁、顺序执行 ops；LevelDB 写 Batch；etcd conditions+operations 一次事务；memory/LevelDB 允许顺序重复键操作，etcd 受协议约束 | 5 以 Go etcd 合法输入为目标；RawTxn 重复键读写在接口声明为 undefined，不新增跨后端承诺 |
| callback 读冲突 | etcd 点读缺失加 CreateRevision=0，存在加 Value=旧值；range 仅为实际返回键追加 Value 条件，不保护未读键、limit 外或插入幻读；同值重写/ABA 可绕过 Value 保护。memory/LevelDB 完全不检查读冲突 | 5 固定值保护和幻读边界；不能把 Go memory 当 serializable oracle |
| revision 读 | etcd LoadWithRevision 使用 caller ctx 读，存在键以 ModRevision=读到的 revision，缺失仍 CreateRevision=0；不把大 value 放进 commit。仅 etcd 实现 RevisionTxn | MetaStore Entry 已含 mod_revision，Compare 已有 ModRevision 等值；6 补 wrapper 并验证同值改写冲突、缺失创建冲突 |
| 附加条件 | etcd RunInTxnWithConditions 复制传入 comparisons，与读保护及 writes 同一提交；失败为 ErrEtcdTxnConflict。endpoint 无条件可回退 Base，有条件而后端不支持则报错 | 5–7 保持原子性，Rust 领域不能暴露 clientv3.Cmp；业务 leader/fencing 条件归上层 |
| 取消 | memory/LevelDB 仅 commit 开始检查 ctx，等待锁期间无二次检查；etcd 点读用 txn.ctx，range 却用 client ctx。etcd commit 用 caller ctx + 10s。CreateRawTxn 用 client ctx；WithContext 版本用 caller ctx + 10s | 11 设计调用方取消与 drop；既有 watch token 不代表所有事务已支持取消 |
| 错误/故障 | etcd get/put/delete 分别包装 ErrEtcdKVGet/Put/Delete；callback compare false 是 ErrEtcdTxnConflict；Raw compare false 是成功响应 false。LevelDB open 包装 ErrLevelDBOpen，Load 缺失单独吞 ErrNotFound，iterator 不读取 Error，commit 直接返回 Write 错误 | 11 保留分类和写结果不确定性，禁止盲重试；LevelDB iterator 行为只记录、不扩展修复 Go |

## RawTxn 六比较、四操作与响应

以下映射直接依据 `rawTxnWrapper.If/convertOps/Commit` 和 `kv_test.go::testRawTxn`。
比较在 etcd 原子提交点求值，多条件为 AND；不是先查询再本地比较。

| 比较类型 | 实际映射与既有断言 | 归属 |
| --- | --- | --- |
| Equal | Value `=`；v1 对 v1 true，对 v2 false | 7 |
| NotEqual | Value `!=`；v1 对 v1 false，对 v2 true | 7 |
| Less | Value `<`；v1 对 v0/v1 false，对 v2 true；字节序，不是数值解析 | 7 |
| Greater | Value `>`；v1 对 v2/v1 false，对 v0 true | 7 |
| Exists | CreateRevision `>0`；缺失 false，存在 true；忽略 Condition.Value | 7 |
| NotExists | CreateRevision `=0`；缺失 true，存在 false，删除后 true；忽略 Value | 7 |

既有 Go 断言特别要求：缺失键对非空 v1 的 Equal/NotEqual/Less/Greater **全部 false**。
现有 Rust MemoryStore 的 Value 等值使用缺失 `[]` 作为默认值，而 EtcdMetaStore 直接委托
etcd Value compare；这使“缺失 == 空值”存在跨后端兼容风险，既有 cas 测试未覆盖。
这不是本轮复现实验：7 必须新增缺失/空值/删除后的真实 etcd 与 memory 同一断言，
不可仅扩展枚举后沿用默认空值语义。缺失与空值的其他组合本轮未获得运行证据。
未知 CmpType 在 Go wrapper panic；非法枚举/请求的 Rust 分类归 11，不泄漏 Go panic。

| 操作/分支 | 真实行为与结果 | 归属 |
| --- | --- | --- |
| Put | OpPut，无 WithPrevKV/lease；对应 response item 为空 | 8、9 |
| Delete | 单键 OpDelete，缺失可删，无 PrevKV；对应 item 为空 | 8、9 |
| Get | 单键 OpGet；存在空值返回一个键值对，缺失返回空 KeyValuePairs | 9 |
| GetRange | sentinel 使用 prefix，否则 WithRange，附 limit；返回 ordered KVs，属于同一事务响应 | 10 |
| Then/Else | 转换后缓存各分支操作列表，Commit 根据 Succeeded 只解码实际分支；false 可执行 Else，不是错误。空条件/空分支及只比较事务也允许 | 7、8–10 |
| 响应对齐 | Responses 与执行分支操作一一同序，包括写操作空 item；校验 response 非 nil、数量相等、类型匹配，否则 ErrEtcdTxnResponse；不能忽略多余或缺少 item | 9–11 |

`RawTxnResponse` 顶部注释写 Put/Delete 返回 previous value，与其字段注释、真实 Commit
和 testRawTxn 的空 item 断言冲突；以实现及断言为准，不能据顶部注释新增 prev value。
未知 OpType 在 convertOps panic；重复对同键读写为接口明确未定义，不建立 read-your-writes
或跨后端重复写等价保证。Go wrapper 依赖底层 client 的 builder 生命周期/重复调用限制，
本轮不推断重复 If/Then/Else 或二次 Commit 的可复用性；非法使用分类归 11。
现有 Rust Commit 只有 succeeded/revision，不返回 item，transact 只有成功分支 writes，
compare 只有 CreateRevision/ModRevision/Value/Lease 的等值；6 复用已有 revision，7 扩比较，
8 加 Else（Else 写入仍须分配 revision），9–10 扩有序响应，不能建立第二套 MetaStore。

## 现有 Rust 能力、测试和装配

canonical 为 `pkg/storage/Cargo.toml` 的 `astersql-pd-storage`，`lib.rs` 重导出
meta_store、MemoryStore、EtcdMetaStore。生产在 `meta_store.rs/memory_store.rs/etcd_store.rs`。
只有这一套公开 MetaStore；独立 `pkg/storage/raft-prototype` 为 durability 原型，不是生产后端。
拟建（当前不存在）：`pkg/storage/kv/Cargo.toml`、`pkg/storage/kv/lib.rs` 与
`astersql-pd-storage-kv`；仅任务 2 移动唯一实现、修改 root manifest/lock 并保留 storage re-export。

既有领域类型穷举：Revision/LeaseId/Result、StoreError、Consistency、Read（prefix/start/limit/
revision/consistency）、Entry（key/value/create_revision/mod_revision/lease）、Snapshot、Compare、
Mutation（Put/Delete）、Commit、Lease、Event、WatchBatch、MetaWatch、MetaStore。
公开辅助方法为 Read::prefix、Mutation::put、Event::{revision,key}；MemoryStore Default/Clone，
EtcdMetaStore::connect。MetaStore 方法 read/transact/watch/grant/keep_alive/revoke/ttl/compact。
现有读支持 revision 和 linearizable/serializable；transact 验证总条件+写入数 <=128、禁止空键/
重复写/非法 lease；compare false 不写、不增 revision；写结果 Unavailable 不能盲重试。
这些是需保留的基础契约，不能以新增 Base API 覆盖掉。

`meta_store_test.rs` 通过 lib 测试模块注册，`etcd_store_test.rs` 通过 Cargo 的
`[[test]] name=etcd_contract` 注册；两者共享 `contract_test.rs` 九个真实断言函数：
cas_atomic_revision（原子 CAS/同 revision/冲突不写/无效删除不增号）、snapshot_scan
（排序/limit/start/历史一致读/all-FF/全表）、watch_replay（起始及批次顺序）、
compaction_recovery（压缩后恢复）、lease_expiry、lease_renew_revoke、election_fencing、
invalid_atomic（重复/空键不产生部分提交）、watch_cancel（进行中与预取消且终止）。
etcd 另有 disconnect_restart_resume_and_cancel，使用 `etcd_fixture_test.rs` 调用
`scripts/rust-etcd-fixture` 随机 loopback 端口、独立目录并在同 WAL 上重启。
这是静态注册清单：9 memory + 9 etcd + 1 fault，并非本轮 19 passed。
任务 12 须再次运行；现有测试不覆盖 Base/callback/六比较/Else/事务读响应，不能拿注册数代替验证。

生产反向依赖：`pkg/member/member.rs`（lease/选主及 fencing）、`pkg/id/allocator.rs`
（原子 ID 分配）、`pkg/runtime/node.rs`（bootstrap/宿主装配）；这些 Cargo.toml 与
`pkg/server/Cargo.toml` 均依赖 storage。`pkg/lib.rs` 仅重导出 runtime API。
member/id 的 `etcd_behavior_test.rs` 以 path 复用 storage fixture，任务 2 必须统一调整，
不得复制 helper；任务 12 回归这些消费者。域层不得因此依赖协议类型、私有 runtime 或全局 ID。
watch 起始 revision/顺序/压缩、lease TTL/续租/到期删除和 fencing 已有 API；四个 Go KV 来源
本身不提供 watch/grant 等接口，Go 上层直接使用 etcd 的行为属 election/member/etcdutil 计划。
事务扩展与这些基础能力的交互回归归 12；新生产引擎、完整服务启动、调度、加密与 PersistOptions
归上层主题，当前盘点完成不能宣称来源史诗完成。

## Go 相邻测试与剩余证据归属

`kv_test.go` 五个顶层测试（不计 TestMain）：TestEtcd、TestLevelDB、TestMemKV、
TestRawTxnWithContext、TestRawTxnWithContextInvalidResponse。前三者共用 testReadWrite、
testRange、testSaveMultiple(20)；只有 etcd 跑 testLoadConflict、testRevisionLoadConflict、
testRunInTxnWithConditions、testRawTxn 和 canceled point read。Raw context 测试断言存在/
缺失/空值区分、预取消和受控 gRPC 阻塞后取消；invalid response 对 Then/Else 各三种
损坏（missing/extra/wrong type）断言 ErrEtcdTxnResponse，不能当作 nil 响应已覆盖。

后续需补运行证据的全部项目：3 范围与特殊上界/非法 limit，4 点读精确性与空值，5 回调
错误回滚、stale read、幻读与值保护，6 同值改写/缺失键 revision，7 六比较及缺失空值，
8 Else 与批次 revision/原子性，9–10 两分支按序结果，11 提交前取消、断连不确定结果/
非法请求/畸形响应，12 原有 watch/lease/fencing/重启/消费者回归。每项均有编号归属；
本轮未运行的 Go 测试不能升级为 oracle 已通过。后续遵循 make gotest 的 failpoint 窗口。

遥测边界：`metrics.go` 的 pd_txn_txns_count、pd_txn_handle_txns_duration_seconds
带 result=success/failed；SlowLogTxn 以 err 是否非 nil 分类（compare false 且无 err 计 success），
超过 1s 输出慢日志并在 Commit 返回后 cancel。指标/日志等价迁移归上层可观测性主题；
不在本子计划增添全局 Registry 或日志订阅器。故障点 etcdSaveFailed、slowTxn、withRangeLimit
及辅助 EtcdKVGet 的 SlowEtcdKVGet，其触发阶段与错误由 3、5、11 的故障验证负责，
全量 failpoint 清单归上层迁移主题。

## Go 直接反向依赖穷举

通过 `rg --files -g '*.go'` 后逐文件检查准确 import 路径（包括别名），
只保留上述公开符号引用；局部变量同名的 Key/Value/Load 等不当作包符号。
这是直接 import 文件清单，接口嵌入后的间接调用由 backend/endpoint 装配传递；
不把整个服务行为归入本次 KV 迁移。测试行是静态消费证据，不是本轮执行结果。

| 直接消费文件（仓库相对） | 公开符号 | 归属 |
| --- | --- | --- |
| `pkg/election/leadership.go` | `NewSlowLogTxn` | election/member/ID 子计划 |
| `pkg/gc/gc_state_manager_test.go` | `NewEtcdKVBase` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/gc/metrics_test.go` | `Base, RawTxn, RawTxnCapable, RawTxnCondition, RawTxnOp, RawTxnResponse, RawTxnWithContextCapable, Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/id/id.go` | `NewSlowLogTxn` | election/member/ID 子计划 |
| `pkg/keyspace/keyspace.go` | `Txn` | 上层业务主题；底层事务 5–11 |
| `pkg/keyspace/keyspace_test.go` | `NewMemoryKV, Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/keyspace/meta_service_group.go` | `Txn` | 上层业务主题；底层事务 5–11 |
| `pkg/keyspace/meta_service_group_test.go` | `NewMemoryKV, Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/keyspace/tso_keyspace_group.go` | `ConditionalTxnRunner, Txn` | 上层业务主题；底层事务 5–11 |
| `pkg/keyspace/tso_keyspace_group_test.go` | `NewEtcdKVBase, NewMemoryKV, Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/keyspace/util_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/mcs/discovery/discover.go` | `NewSlowLogTxn` | 上层 MCS/TSO/同步/启动装配主题 |
| `pkg/mcs/resourcemanager/server/keyspace_manager.go` | `Txn` | 上层 MCS/TSO/同步/启动装配主题 |
| `pkg/mcs/resourcemanager/server/keyspace_manager_test.go` | `Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/mcs/resourcemanager/server/manager.go` | `NewEtcdKVBase, Txn` | 上层 MCS/TSO/同步/启动装配主题 |
| `pkg/mcs/resourcemanager/server/manager_test.go` | `Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/mcs/scheduling/server/affinity/watcher_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/mcs/scheduling/server/grpc_service_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/mcs/scheduling/server/rule/watcher_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/mcs/scheduling/server/server.go` | `NewMemoryKV` | 上层 MCS/TSO/同步/启动装配主题 |
| `pkg/mcs/scheduling/server/server_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/mcs/utils/expected_primary.go` | `NewSlowLogTxn` | election/member/ID 子计划 |
| `pkg/member/member.go` | `NewSlowLogTxn` | election/member/ID 子计划 |
| `pkg/schedule/affinity/txn.go` | `Txn` | 上层业务主题；底层事务 5–11 |
| `pkg/schedule/labeler/keyspace_index_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/schedule/labeler/labeler.go` | `Txn` | 上层业务主题；底层事务 5–11 |
| `pkg/schedule/labeler/labeler_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/schedule/labeler/plan.go` | `Txn` | 上层业务主题；底层事务 5–11 |
| `pkg/schedule/labeler/plan_test.go` | `NewMemoryKV, Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/schedule/placement/rule_manager.go` | `Txn` | 上层业务主题；底层事务 5–11 |
| `pkg/schedule/placement/rule_manager_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/storage/endpoint/affinity.go` | `Txn` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/endpoint.go` | `Base, ConditionalTxnRunner, RawTxn, RawTxnCapable, RawTxnWithContextCapable, Txn` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/gc_state_batch.go` | `RawTxnOp, RawTxnOpGet` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/gc_state_batch_test.go` | `NewEtcdKVBase, NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/storage/endpoint/gc_states.go` | `RawTxnCmpEqual, RawTxnCmpNotExists, RawTxnCondition, RawTxnOp, RawTxnOpDelete, RawTxnOpPut` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/gc_states_test.go` | `KeyValuePair, NewEtcdKVBase` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/storage/endpoint/keyspace.go` | `RevisionTxn, Txn` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/maintenance.go` | `RawTxnCmpEqual, RawTxnCmpNotExists, RawTxnCondition, RawTxnOp, RawTxnOpDelete, RawTxnOpGetRange, RawTxnOpPut` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/meta_service_group.go` | `Txn` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/rule.go` | `Txn` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/tso.go` | `Txn` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/tso_keyspace_group.go` | `RevisionTxn, Txn` | endpoint 子计划；底层 3–11 |
| `pkg/storage/endpoint/tso_keyspace_group_test.go` | `NewEtcdKVBase, Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/storage/endpoint/util.go` | `Txn` | endpoint 子计划；底层 3–11 |
| `pkg/storage/etcd_backend.go` | `NewEtcdKVBase` | storage facade 子计划；归属移动 2 |
| `pkg/storage/hot_region_storage.go` | `LevelDBKV, NewLevelDBKV` | storage facade 子计划；归属移动 2 |
| `pkg/storage/keyspace_test.go` | `Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/storage/leveldb_backend.go` | `LevelDBKV, NewLevelDBKV` | storage facade 子计划；归属移动 2 |
| `pkg/storage/memory_backend.go` | `NewMemoryKV` | storage facade 子计划；归属移动 2 |
| `pkg/storage/meta_service_group_test.go` | `Txn` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/storage/region_storage.go` | `Base` | storage facade 子计划；归属移动 2 |
| `pkg/storage/region_storage_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/storage/storage.go` | `Base, ConditionalTxnRunner, Txn` | storage facade 子计划；归属移动 2 |
| `pkg/syncer/history_buffer.go` | `Base` | 上层 MCS/TSO/同步/启动装配主题 |
| `pkg/syncer/history_buffer_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `pkg/syncer/server.go` | `Base` | 上层 MCS/TSO/同步/启动装配主题 |
| `pkg/tso/keyspace_group_manager.go` | `NewEtcdKVBase` | 上层 MCS/TSO/同步/启动装配主题 |
| `server/grpc_service.go` | `NewSlowLogTxn` | 上层 MCS/TSO/同步/启动装配主题 |
| `server/microservice_cleanup.go` | `NewSlowLogTxnWithContext` | 上层 MCS/TSO/同步/启动装配主题 |
| `server/microservice_cleanup_test.go` | `Txn` | 相关生产消费者的测试；KV 回归 12 |
| `server/server.go` | `Base, NewSlowLogTxn, Txn` | 上层 MCS/TSO/同步/启动装配主题 |
| `server/server_test.go` | `Txn` | 相关生产消费者的测试；KV 回归 12 |
| `tests/integrations/mcs/resourcemanager/resource_manager_test.go` | `Txn` | 相关生产消费者的测试；KV 回归 12 |
| `tests/integrations/mcs/scheduling/config_test.go` | `NewMemoryKV` | 相关生产消费者的测试；KV 回归 12 |
| `tests/integrations/mcs/tso/server_test.go` | `Txn` | 相关生产消费者的测试；KV 回归 12 |
| `tests/server/api/hot_status_test.go` | `LevelDBKV` | 相关生产消费者的测试；KV 回归 12 |

## 本轮验证结果

符号扫描按任务原命令执行：

```sh
rg -n '^(func |type |pub |    pub )' pkg/storage/kv/kv.go pkg/storage/kv/etcd_kv.go pkg/storage/kv/mem_kv.go pkg/storage/kv/levedb_kv.go
```

| 检查 | 确切结果 |
| --- | --- |
| 上述符号扫描命令 | 退出 0；随后补读全部函数体及多行签名 |
| `git diff 5d5948052a9ee9e3fa7a09145fe2916a2912c223 -- pkg/storage/kv/kv.go pkg/storage/kv/etcd_kv.go pkg/storage/kv/mem_kv.go pkg/storage/kv/levedb_kv.go` | 退出 0，无差异 |
| Python 逐文件 import/公开符号与路径核对 | 退出 0；67 个直接 import 文件，全部公开类型和方法有清单；73 个明确源码路径及 4 个 Markdown 链接存在；2 个拟建 KV 路径确认不存在并已标注 |
| `git diff --check` | 退出 0 |
| `git diff --cached --check` | 退出 0，包含新增证据文档的实际暂存内容 |

人工已逐项对照六比较、四操作、Else、范围上界、取消、错误、Go 内存/etcd 差异，
并审查证据文档及完成任务清理差异。仅新增本文并清理本编号文件；只读计划/来源主题
与代码没有本任务差异，无关元数据观测编号文件的既有改动不纳入提交。
本任务明确禁止 Go/Cargo 测试，故有效运行测试数不适用、未领取 Cargo 槽位；
未运行测试是文档任务边界，不是待回归实现，也不据此宣称缺口已补齐。

## 任务 2：唯一 MetaStore 下沉（批次 9）

日期：2026-10-23（Asia/Shanghai）。状态：已完成。准入：批次 8 的提交 `1e37a3529`
及本文任务 1 的实际产物已核验；前序编号文件删除不单独视为完成证据。
本轮从 `1e37a3529` 开始，四个 Go KV 来源相对规划基准仍无差异。
只读本计划 plan.md 与来源主题，未修改其内容。

### 归属、类型与行为

唯一 canonical crate 改为 `astersql-pd-storage-kv`，无额外 feature。
`meta_store.rs`、`memory_store.rs`、`etcd_store.rs` 从 storage 移到 kv，
与迁移前 `git show HEAD:pkg/storage/<文件>` 的字节完全相同。
kv/lib.rs 注册这三个模块并导出原 API；storage/lib.rs 使用
`pub use astersql_pd_storage_kv::*`，没有 wrapper、平行 trait 或第二套实现。
因此 Revision、LeaseId、Result、StoreError、Consistency、Read、Entry、Snapshot、
Compare、Mutation、Commit、Lease、Event、WatchBatch、MetaWatch、MetaStore、
MemoryStore、EtcdMetaStore 均仍是同一份定义，公开字段、方法与错误变体未变。
原有 crate 内 validate_txn 路径无需改写，模块整体移动后仍有效。

member/ID/TSO 的生产与测试直接消费 kv；runtime 等原 storage 消费方继续通过重导出，
其真实装配把 storage::MetaStore 对象交给消费 kv::MetaStore 的 member/ID/TSO，
由编译和现有行为测试共同核对类型身份。当前尚无 endpoint Rust crate，本任务没有
提前创建它；本步依赖为 storage → kv，后续 endpoint 实现时形成 storage → endpoint → kv。
kv 不依赖 storage。未增加事务 API、引擎、协议暴露或业务算法。

四个测试/fixture 文件一并移到 kv，契约断言仍只有一份：
`contract_test.rs` 的九个函数由两个 crate 的内存测试和 etcd_contract target 复用；
storage 保留原 meta_store_test 模块和 etcd_contract target 的名称及测试名称，
只调整显式 path。新 kv crate 使用相同注册，未新增虚构的行为缺陷或空测试。
本任务采用计划允许的 refactor 既有入口，无需人为改名 metastore_move_*。
共享 fixture 的类型 import 改为 kv，按消费者 manifest 的祖先定位原 fixture 源目录；
member/ID/TSO/runtime/server/命令行/兼容 harness 的 path 与依赖同步调整。
未改 Go fixture、随机 loopback 端口、独立目录、同 WAL 重启和 Drop 清理生命周期。
兼容 harness 是库的公开模块，因此其 kv 依赖为普通依赖，其余新增 fixture 依赖为 dev 依赖。
Cargo.lock 只新增本地 kv 包并调整上述依赖边，没有第三方版本变化。

### Go 依据与本轮运行

读取 `kv_test.go::TestEtcd/TestMemKV/testReadWrite/testRange/testSaveMultiple`：
缺失/删除 Load 为空，Save 后可读原值，重复删除成功；范围按字节排序，正 limit 截断；
etcd 特殊 NUL 上界与内存差异保持原样。TestEtcd 还验证 stale read/revision 冲突、
附加条件、RawTxn 比较及响应和预取消 Load。错误契约与尚未补齐能力仍由任务 1 表格负责；
本任务不把 Go Base 与现有 Rust MetaStore 声称为逐项等价。

实际运行 `make gotest GOTEST_ARGS='./pkg/storage/kv -run TestEtcd\|TestMemKV -count=1 -v'`，
退出 0，TestEtcd/TestMemKV 两个顶层测试通过；之后 `make failpoint-disable` 退出 0。
首次筛选用了尾部 `$`，被 make 展开导致 shell 引号不完整，退出 2，测试未执行；
关闭 failpoint 后修正筛选并成功重跑。最终无 Go 文件差异、无生成 failpoint binding 文件。
本任务不改变编码，未创建字节 oracle，也不声称完成跨语言字节差分。

### Cargo 验证与共享槽位

迁移前阶段、首次格式检查、依赖修复检查及最终完整阶段均依次原子 mkdir 领取实际槽位 1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁目录为 target/rust-slot-locks/slot-1.lock，owner 记录计划、任务、会话和持有 Python PID；
所有 Cargo/fixture 继承上述绝对路径。每阶段等待子进程结束后删除自己的 owner 并 rmdir，
保留缓存；未清理其他锁或缓存。最终阶段的中断处理先终止并等待子进程组，再释放锁。

确切命令与结果：

| 命令 | 退出码 | 有效测试数/结果 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-storage -p astersql-pd-member -p astersql-pd-id -p astersql-pd-tso -- --nocapture`（迁移前） | 0 | 45 passed / 0 failed / 0 ignored |
| `cargo fmt --all`（最终修复后） | 0 | 格式化完成，无无关 Rust 格式变更 |
| `cargo check --workspace`（同步锁文件，最终修复后） | 0 | 已审查仅本任务依赖变化 |
| `cargo test --locked -p astersql-pd-storage -p astersql-pd-member -p astersql-pd-id -p astersql-pd-tso -- --nocapture`（迁移后） | 0 | 45 passed / 0 failed / 0 ignored，名称及重复次数与迁移前一致 |
| `cargo test --locked -p astersql-pd-storage-kv -- --nocapture` | 0 | 19 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace 注册正确 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 478 passed / 0 failed / 1 既有 ignored |
| `git diff --check` | 0 | 通过 |
| `git diff --cached --check` | 0 | 暂存后通过 |

聚焦测试构成：ID 2+5、member 2+5、storage 9+10、TSO 3+9，共 45。
新 kv 复用 9 memory + 9 etcd + 1 fault；零测试 doc-test 不计入有效测试数。
首次格式化发现四个额外 fixture 消费者仍引用旧路径（退出 1）；补齐其 path 后，
首次 workspace check 发现兼容 harness 的普通库模块缺少 kv 普通依赖（退出 101）；
调整依赖类别后全部聚焦检查通过。这些是本任务迁移路径遗漏，均已在范围内修复。

workspace 中唯一既有 ignored 为兼容套件的 `tso_processes`，其要求独立 Go/Rust
进程二进制与 scripts/run-pd-compat.sh；未将该测试计入 478 个通过场景。
本任务所需双后端契约、消费者、WAL 重启与故障入口均实际执行，无新增 ignore。

故障测试时序：写入 revision 2，lease 写入 revision 3 并由 watch 观察后停 etcd；
watch/read 返回 Unavailable，取消 down-endpoint watch 返回 Cancelled；等待 3 秒，
同 WAL 重启并写入 revision 4，原 watch 从 revision 3 后续读观察到新提交；
最终等待 lease TTL 返回 LeaseNotFound、绑定键消失且 keep_alive 返回 LeaseNotFound。
迁移前、迁移后及新 kv 三次均打印 resumed after 3 / persisted commit 4 / lease expired；
故障中的 lease ID 为随机值，不按字面比对。自然到期、续租、撤销及 fencing 的共享断言
也在两后端实际通过。

最终检查：生产三文件逐字节一致；迁移前后 45 个测试名称的多重集合一致（不要求
异步完成顺序一致）；共享断言/fixture 各一份；所有文件差异已审查，plan.md、来源主题、
Go 与无关观测任务文件不纳入提交。全部子进程结束，slot-1.lock 已释放，缓存保留。
任务编号文件按完成流程删除，证据保留于本文及本任务实现提交。

## 任务 3：有界范围读取（批次 10）

日期：2026-10-23（Asia/Shanghai）。状态：已完成。
准入：提交 `56097c4f1` 的唯一 `astersql-pd-storage-kv`、storage 重导出、共享
fixture 和本文任务 2 的双后端/消费者证据均已核验，批次 9 满足。
读取本任务、只读 plan.md、ADR-001、移植指南、根 AGENTS.md 与 Copilot 指令；
仓库没有更深层 AGENTS.md。无关观测任务文件的既有改动不纳入本任务。
`etcd_kv.go` 相对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

### Go 依据与行为边界

`etcdKVBase::LoadRange` 对普通上界调用 `clientv3.WithRange(endKey)`，
起点包含、终点排除，WithLimit(0) 不限；只有上界恰为单字节 NUL 才改用 WithPrefix，
此时空起点表示全表。`kv_test.go::testRange` 固定全表、任意区间、正 limit、
不存在的起点与跨 prefix 范围，且主动跳过 Go 内存后端的 NUL 上界场景。
本任务两个 Rust 后端统一采用 Go etcd 的选择语义。

本地固定 Go etcd client v3.5.15 的 `op.go::{WithRange,WithPrefix,getPrefix}`
也经核对：空 end 保持点读；空 prefix 变成 `[NUL,NUL)` 全表请求；全 FF prefix
使用 NUL 表示没有有限上界。普通含 NUL 的 end（如 `a\x00`）仍是普通排除上界。
空/反向区间、空 end、非法空起点的额外 Rust 断言结合来源推导及真实 etcd 运行验证；
不把这些额外场景声称为 Go 测试已逐项执行的跨语言差分。
没有编码变化，未创建字节 oracle。

实际先运行 `make gotest GOTEST_ARGS='./pkg/storage/kv ./pkg/storage -count=1'`，
退出 0，两个包均通过（KV 3.780s、storage 27.680s；命令未启用 -v，输出未列用例总数）。
随后 `make failpoint-disable` 退出 0。Rust 编辑与验证阶段无 Go 文件差异，
没有生成 failpoint binding 文件留在工作区。

### 唯一实现与测试驱动证据

原位扩展 `meta_store.rs::Read`，新增可选排除上界 `end` 和 `Read::range`，
保留既有 prefix、start、revision、consistency、limit 行为，不创建平行请求或 Base 实现。
range 构造器规范化 NUL prefix sentinel；空 end 表示精确点读。
校验拒绝空 bounded start、混用非空 prefix 与 bounded end、非法 revision 和溢出 limit。
`etcd_store.rs::read` 将边界和 limit 直接下推 etcd；
`memory_store.rs::read` 从 BTree 范围起点定位，在上界停止，只收集 limit 项并探测 more，
不再扫描/克隆整个键表后截断。默认 features、manifest 与 Cargo.lock 均未修改。

独立 `bounded_range_test.rs` 在 lib.rs 显式注册，并复用唯一 etcd fixture。
最小可编译接口先沿用旧 prefix 选择，运行计划指定聚焦命令：
内存和真实 etcd 都在 `[a,c)` 的键集合断言失败，实际只返回 `a,a\x00,aa`，
预期还包含 `b,ba`。真实状态红灯为 0 passed / 2 failed，退出 101；
不是缺符号或环境失败。修复后相同两个测试和原断言均通过。
后续增加请求构造值测试与请求校验、prefix 分页回归，最终聚焦 3 passed。

每个后端共用 13 个选择场景 × 2 种 consistency：起点包含、终点排除、跨 prefix、
起点不存在、零 limit、正 limit、more、刚好 limit、相等/反向区间、空结果、
NUL prefix、空 prefix 全表、全 FF prefix、普通含 NUL 上界、空 end 存在/缺失点读。
逐项断言升序 keys、value 对齐、header revision、mod_revision 与 more；
另验证历史 revision 的旧值/边界、原 prefix 分页及五种非法请求。

### 确切验证与槽位

红灯、首次绿灯、最终完整阶段均动态从 1–10 原子 mkdir 领取实际槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、任务、会话标识及持有 Python PID（最终阶段 23133）；
每阶段 Cargo 和 fixture 全部继承绝对路径。包装器在正常/失败/中断路径等待子进程结束，
中断时终止并等待子进程组，然后删除自己的 owner 并 rmdir；未修改其他锁/缓存。
全部阶段完成后确认锁目录为空，槽位编译缓存保留。

| 确切命令 | 退出码 | 有效测试数/结果 |
| --- | --- | --- |
| `cargo fmt --all`（红灯前、修复后、最终阶段） | 0 | 无无关 Rust 格式变化 |
| `cargo test --locked -p astersql-pd-storage-kv --lib bounded_range -- --nocapture`（红灯） | 101 | 0 passed / 2 failed，双后端相同键集合断言 |
| 同上（首次绿灯） | 0 | 2 passed / 0 failed / 0 ignored |
| 同上（最终） | 0 | 3 passed / 0 failed / 0 ignored；两个后端入口均命中 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace 注册正常，输出定向到临时文件审查 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 481 passed / 0 failed / 1 既有 ignored（479 单元/集成 + 2 doc-tests） |
| `git diff --check` | 0 | 通过 |
| `git diff --cached --check` | 0 | 暂存后通过 |

既有 ignored 为 `tso_processes`（需要独立 Go/Rust 进程与兼容脚本）；本任务未新增
ignore。workspace 仅输出已有 RegionHeartbeatRequest.cpu_usage deprecated 警告。
没有新断连/到期行为；完整基线仍实际运行现有 watch/lease/fencing/重启测试。
已审查本任务生产、独立测试、注册和证据差异；plan.md、来源主题、Go、manifest/lock
均保持原样。编号任务文件按完成流程清理，证据保留在本文和实现提交中。

## 任务 4：基础单键读写（批次 11）

日期：2026-10-23（Asia/Shanghai）。依赖批次 10 已核验：提交 `2cfb05af8`
包含唯一 Read 的有界/精确读取、双后端测试及本文任务 3 的完整验证证据，准入满足。
已读 ADR-001、porting-guide、根 AGENTS 与 Copilot 指令；无更深 AGENTS。
`kv.go`、`etcd_kv.go` 对规划基准 `5d5948052a9ee9e3fa7a09145fe2916a2912c223`
无差异。本轮仅实现 Base 单键行为，未修改总计划或来源主题。

### Go 依据与实现

`etcd_kv.go::Load` 使用精确 EtcdKVGet，缺失返回空字符串，多条响应报错；
`Save` 通过无条件事务 OpPut 覆盖单键、不绑定 lease；`Remove` 使用单键 OpDelete，
允许删除缺失键。`kv_test.go::testReadWrite` 在三个 Go 后端共同断言缺失、写读、删除、
重复删除；覆盖/空值/精确前缀及字节透明性来自上述源码推导和本轮真实 etcd 测试，
不声称这些新增 Rust 场景都已经由 Go 测试逐项运行差分。

本轮先运行 `make gotest GOTEST_ARGS='./pkg/storage/kv ./pkg/storage -count=1'`，
退出 0，两包通过（4.102s、28.079s）；输出未列 Go 测试总数。
随后 `make failpoint-disable` 退出 0，确认无 Go 差异及生成 failpoint binding 文件。
Go failpoint 窗口结束后才开始 Rust 编辑与 Cargo 阶段。

新增 `base_kv.rs::Base::{new,load,save,remove}`，借用 canonical `dyn MetaStore`。
Load 复用 `Read::range(key, empty_end, Linearizable)` 点读，保留缺失与存在空值的
Base 返回语义；Save/Remove 各提交一个无条件 Mutation，传递既有 StoreError 分类。
异常多条点读响应或无条件事务返回 false 防御性归类 Backend；不新增错误分类、
重试、后台任务或运行时。底层存在空值仍保留 Entry，未混淆其事务比较语义。
没有编码转换，没有新增字节 codec/oracle、依赖、manifest 或 lock 变更。

### 测试驱动证据

独立 `base_kv_test.rs` 在 lib.rs 显式注册，两个入口 `base_kv_memory` 与
`base_kv_etcd` 共用完整断言和现有唯一 etcd fixture。
最小可编译实现先复用旧 prefix 读取，聚焦命令实际退出 101：0 passed / 2 failed。
两个后端均在只有 `key/adjacent=neighbor` 时断言 Load(`key`) 返回空值失败，
实际返回 neighbor。这是相邻键误读的真实状态红灯。改为精确读取后，相同测试原断言
未削弱，2 passed / 0 failed / 0 ignored，退出 0。

共用断言覆盖：缺失读/删、同前缀邻键不能命中、精确写读、覆盖、存在空值及实际 Entry、
无 lease、精确删除/重复删除/邻键保留、非 UTF-8 与 NUL 字节往返、空键三操作的
InvalidArgument。字节预期直接来自输入，属于透明性测试，不是编码跨语言差分。
没有新增断连或到期故事；原有 watch/lease/fencing/重启由 workspace 基线回归。

### 验证与 Cargo 资源

两个阶段均通过原子 mkdir 动态取得实际槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
占用记录含计划、任务、会话和持有 PID（红灯 28660，绿灯及基线 28809）。
Cargo 及 fixture 子进程继承此绝对路径。包装器正常/失败退出等待子进程结束后清理
owner 与自己的锁；最终阶段处理中断信号，终止并等待子进程组后再释放槽位。
保留槽位缓存，不清理他人锁或构建目录。

首次 workspace 回归退出 101：旧 `bounded_range_etcd` 在 header revision 断言上
收到 3、预期 2，而 Base 两入口均通过。排查发现两个 lib 测试模块分别用 path
引入相同 fixture 源码，形成独立 NEXT 静态计数器；同一 PID 的目录名发生碰撞，
实际复用对方数据目录/ready 文件。这是本次新增入口触发的测试装配问题。
最小修复在 lib.rs 注册一次共享 fixture；Base 与 bounded_range 测试引用同一模块，
后者只改 fixture import，生产逻辑与原有断言均未改动。不复制 helper 或修改 fixture。
最终验证阶段重新动态领取槽位 1，持有 PID 31013，路径同上。

最终状态：已完成。最终阶段所有命令退出 0，自己的锁已释放，槽位缓存保留。

| 确切命令 | 退出码 | 有效测试数/结果 |
| --- | --- | --- |
| `cargo fmt --all`（各次 Rust 修改后） | 0 | 无无关 Rust 格式变更 |
| `cargo test --locked -p astersql-pd-storage-kv --lib base_kv -- --nocapture`（红灯） | 101 | 0 passed / 2 failed，双后端相邻键误读 |
| 同上（首次绿灯及最终阶段） | 0 | 每次 2 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | canonical crate 注册正常 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked`（最终） | 0 | 483 passed / 0 failed / 1 既有 ignored，含 2 doc-tests |
| `git diff --check` | 0 | 通过 |
| `git diff --cached --check` | 0 | 暂存审查通过 |

既有 ignored 为需独立兼容流程的 `tso_processes`；没有新增 ignore。
仅有既有 RegionHeartbeatRequest.cpu_usage deprecated 警告。
已审查新增 Base、共用断言、模块注册、最小 fixture import 修复及本文证据。
Go、manifest/lock、plan.md 和来源主题未改动；原有其他计划修改未纳入提交。

## 任务 5：回调事务提交（批次 12）

日期：2026-10-23（Asia/Shanghai）。依赖批次 11 准入已核验：提交
`fcd4f83cc` 包含 Base 单键读写、两个后端测试入口及本文任务 4 的完整证据；
其最终 workspace 为 483 passed / 0 failed / 1 既有 ignored。
本轮已读取只读 plan.md、任务文件、ADR-001、porting-guide、根 AGENTS 与
Copilot 指令；无更深 AGENTS。`etcd_kv.go` 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

### Go 依据与实现边界

`etcd_kv.go::RunInTxnWithConditions` 先调用回调，错误原样返回且不提交；
Save/Remove 只追加操作。`loadValue(false)` 对缺失键追加 CreateRevision=0，
对存在键追加 Value=读到的字节；`LoadRange` 只保护实际返回的键值。
`commit` 将所有条件与写入一次提交，compare 失败返回 ErrEtcdTxnConflict。
这不是快照隔离：读直接访问后端，不读本事务暂存写；同值改写可以通过，范围新增键、
空范围和 limit 外的键不受保护。未添加 Go 未保证的幻读保护或 revision 保护。

本轮先串行执行 `make gotest GOTEST_ARGS='./pkg/storage/kv ./pkg/storage -count=1'`，
退出 0：KV 包 3.595s、storage 包 27.800s；输出未列测试总数。
实际运行覆盖 `TestEtcd` 调用的 `testSaveMultiple`、`testLoadConflict`、
`testRunInTxnWithConditions` 等相邻断言。随后 `make failpoint-disable` 退出 0，
无 Go 源码差异或生成 failpoint 文件。Go 窗口结束后才编辑 Rust、运行 Cargo。
上述额外边界来自明确的 Go 源码推导和 Rust 双后端实测，不冒充逐场景 Go/Rust 运行差分。
本任务无 codec，不创建字节编码 oracle，不修改 Go、依赖、manifest 或 Cargo.lock。

新增 `callback_txn.rs::CallbackTxn`，复用唯一 `dyn MetaStore`；为 Base 增加
`run_in_txn` 与 `run_in_txn_with_conditions`。异步回调返回成功值后才调用一次
MetaStore::transact，失败丢弃缓冲区；无自动重试。`CallbackTxnError<E>` 保留
原始回调错误 E，分别表示 Callback(E)、Store(StoreError) 和 Conflict。
CallbackTxn 提供 save/remove/load/load_range，读写使用字节及领域类型。
Base 的 store 字段仅扩大到 crate 内可见，以便原位扩展；lib.rs 显式重导出 API
并注册独立测试；未增加 MetaStore 副本、后台任务或私有 runtime。

### 测试驱动与行为证据

`callback_txn_test.rs` 的 `callback_txn_memory` / `callback_txn_etcd` 共用完整断言，
etcd 使用 canonical fixture，不复制 helper。最小可编译接口最初只执行回调而未提交；
聚焦命令真实退出 101，0 passed / 2 failed：成功回调后读取 `new` 得到空字节，
预期 `committed`。这是状态断言红灯，不是缺符号或环境错误。
补上原子提交后，同一测试未削弱断言，退出 0：2 passed / 0 failed / 0 ignored。

双后端断言覆盖：

- 回调错误保留 sentinel，暂存 put/delete 均不写入，revision 和完整 Entry 不变；
  点读/范围读看不到缓冲 put/delete。
- 成功回调返回原值，多键 put/delete 一次提交，写入具有相同的新 revision。
- 读值后发生外部改写，或缺失键被外部写入空值，提交返回 Conflict；
  缓冲写与删除均不生效，失败不额外消耗 revision。
- 同值改写仍允许提交；范围已返回项变化会冲突，limit 外改写、新增幻读和空范围插入
  仍允许提交。交错通过回调内直接等待外部 Base 写入完成后再返回来确定，无时间猜测。
- 附加比较失败不写入，成功才提交；无效写请求作为 Store(InvalidArgument) 返回，
  批次中的合法写也不落地；读错误经回调返回时保持 Callback(InvalidArgument)。

### Cargo 共享槽位与验证

每个阶段通过原子 mkdir 动态领取槽位 **1**；绝对路径均为
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、任务、会话标识和实际持有 PID：首次格式化 37082、红灯 37119、
实现后格式化 37336、绿灯 37349、workspace 基线 38008。
包装器处理正常/失败/中断退出，等待 Cargo 与 fixture 子进程结束再移除自己的 owner
及锁目录；处理中断时终止并等待进程组。不清理他人锁，保留编译缓存。

最终状态：已完成。所有基线命令退出 0，自己的槽位锁已释放、缓存保留。

| 确切命令 | 退出码 | 有效测试数/结果 |
| --- | --- | --- |
| `cargo fmt --all`（每次 Rust 修改后） | 0 | 无无关 Rust 格式变更 |
| `cargo test --locked -p astersql-pd-storage-kv --lib callback_txn -- --nocapture`（红灯） | 101 | 0 passed / 2 failed，成功回调后未提交 |
| 同上（绿灯） | 0 | 2 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | canonical crate 注册正常 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 485 passed / 0 failed / 1 既有 ignored，含 2 doc-tests |
| `git diff --check` | 0 | 通过 |
| `git diff --cached --check` | 0 | 本任务暂存审查通过 |

既有 ignored 仍为独立兼容流程的 `tso_processes`，未新增 ignore。
仅出现既有 RegionHeartbeatRequest.cpu_usage deprecated 警告。
已审查生产 API、两个后端共用断言、Base 最小连线和本文证据；未修改只读计划或来源主题，
其他计划中的在途修改未纳入本任务提交。

## 任务 6：修订号保护读（批次 13）

日期：2026-10-23（Asia/Shanghai）。依赖批次 12 已核验：提交 `c0cf5d626`
包含 canonical CallbackTxn、独立双后端测试及本文任务 5 的完整证据，最终
workspace 485 passed / 0 failed / 1 既有 ignored；准入满足。
本轮读取只读 plan.md、任务文件、ADR-001、porting-guide、根 AGENTS.md 和
Copilot 指令；仓库无更深 AGENTS.md。`etcd_kv.go` 相对规划基准
`5d5948052a9ee9e3fa7a09145fe2916a2912c223` 无差异。

### Go 依据与实现

`etcdTxn::LoadWithRevision` 调用 `loadValue(key,true)`：缺失键返回空字节并附加
CreateRevision=0，存在键返回其原值并附加 ModRevision=读取的 mod revision。
普通 Load 使用 Value 比较，因此同值改写仍可提交；revision 读会冲突且整批不写。
`kv_test.go::testRevisionLoadConflict` 先读取 value，再外部写回相同 value，断言
ErrEtcdTxnConflict 和 resultKey 未创建；TestEtcd 实际调用此断言。

本轮先运行 `make gotest GOTEST_ARGS='./pkg/storage/kv ./pkg/storage -count=1'`，
退出 0，两包通过（KV 3.671s、storage 28.806s；未启用 -v，输出未列测试总数）。
随后 `make failpoint-disable` 退出 0，无 Go 差异或生成 failpoint binding 文件。
Go 窗口结束后才编辑 Rust、运行 Cargo。其余边界来自明确 Go 源码推导及本轮
双后端实测，不冒充逐场景 Go/Rust 运行差分。没有 codec，不创建字节 oracle。

新增 `revision_txn.rs::CallbackTxn::load_with_revision`，在现有 canonical 事务上
扩展方法，不新建第二种事务或 MetaStore。`callback_txn.rs` 只抽取共享的
`load_value` 点读路径，按调用模式附加 ModRevision 或 Value，缺失条件和错误
处理共用。revision 条件只携带 key/revision，不携带 value；仍返回原始字节。
lib.rs 显式注册生产和独立测试模块，无依赖、manifest 或 Cargo.lock 变化。

### 测试驱动与边界证据

独立 `revision_txn_test.rs` 的 `revision_txn_memory` / `revision_txn_etcd` 共用
同一断言，复用唯一 etcd fixture。最小可编译方法先委托普通 load；测试最初有
Snapshot 不支持 PartialEq 的编译错误，改为分别断言 entries/revision 后再运行。
该编译错误不计行为红灯。真正红灯为两个后端均实际返回 Ok(())、预期 Conflict，
聚焦命令退出 101，0 passed / 2 failed；原因为同值改写未被 revision 读检测。
修复后同一测试、原断言未削弱，退出 0：2 passed / 0 failed / 0 ignored。

oneshot 屏障和 tokio::join! 固定顺序：事务读完成，外部同值写入完成，才允许
回调结束和提交，不使用 sleep 或核心事务 mock。普通读允许提交，revision 读拒绝；
断言被拒批次的 put/delete 均未生效，并且冲突不额外分配 revision。
其他共用断言覆盖值变化、删除、删除后同值重建、缺失到空值/非空值创建；每例冲突后
完整 Entry 集合及 header revision 等于外部写入后的快照。稳定值、空值、256 KiB
二进制值及缺失读可成功提交；附加比较仍参与原子提交；读看不到缓冲写；空 key 的
InvalidArgument 保留为 Callback 错误，缓冲写不提交。没有新增断连/到期行为。

### Cargo 资源与验证

各阶段依次通过原子 mkdir 动态领取实际槽位 **1**，绝对路径均为
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner 记录计划、任务、会话和持有进程 PID；格式化 PID 为 43612、43732、43901，
最终聚焦 PID 43899，workspace 阶段 PID 44056。所有 Cargo 及 fixture 子进程继承
绝对路径。包装器正常/失败时等待子进程完成，处理中断时终止并等待子进程组，再删除
自己的 owner、rmdir 释放锁；不清理他人锁，保留槽位缓存。

最终状态：已完成。全部 Cargo/fixture 已结束，slot-1.lock 已释放，缓存保留。

| 确切命令 | 退出码 | 有效测试数/结果 |
| --- | --- | --- |
| `cargo fmt --all`（每次 Rust 修改后） | 0 | 无无关 Rust 格式变更 |
| `cargo test --locked -p astersql-pd-storage-kv --lib revision_txn -- --nocapture`（行为红灯） | 101 | 0 passed / 2 failed，同值改写未冲突 |
| 同上（修复后绿灯） | 0 | 2 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | canonical kv 注册正常 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 487 passed / 0 failed / 1 既有 ignored，含 2 doc-tests |
| `git diff --check` | 0 | 通过 |
| `git diff --cached --check` | 0 | 本任务暂存审查通过 |

既有 ignored 仍为 `tso_processes`，需要独立 Go/Rust 进程兼容流程，未新增 ignore。
workspace 实际通过既有 watch/lease/fencing、断连/WAL 重启与消费者测试；
仅输出既有 RegionHeartbeatRequest.cpu_usage deprecated 警告。
已审查新增方法、共用点读分支、独立测试、模块注册与本文差异；未修改 Go、依赖、
manifest/lock、只读 plan.md 和来源主题，未纳入其他计划的既有修改。
编号任务文件按完成流程删除，证据保留于本文和本任务提交。

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

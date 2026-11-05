# 任务040：键空间元数据移除

状态：已完成，2026-11-05，基准51a061c86。
[task-040-evidence.json.gz](task-040-evidence.json.gz)保存原始阶段、Go叶例、在线差分/etcd事件及输入摘要。

KeyspaceManager提供owned remove与PreparedKeyspaceRemoval：准备阶段使用真实事务读取metadata revision，
检查保护ID/状态，缓冲name与meta删除及meta-service assignment递减；drop或提交拒绝不会发布缓存变化。
commit成功后释放metadata锁，再删除canonical Cache item并通知宿主GCBarrierInvalidator。
observer只允许同步本地cache/metrics失效，不能发存储IO；库不创建GC manager、后台任务或私有runtime。
内部stage_remove_if_eligible是严格与批量过滤共用的唯一算法，包含Skipped/Deferred/Staged结果，供42复用。
严格模式拒绝Enabled/Disabled，允许Archived/Tombstone及Go允许的未知状态；批量模式只接收Archived/Tombstone，
跳过protected/missing/其他状态，predicate不接受时defer且不缓冲删除。

只读来源keyspace.go:1077–1174。Go测试使用的RemoveKeyspace(txn,id)在调用者commit前就清缓存；
生产bulk路径则在批次提交后发布。Rust本项按目的提供owned提交/receipt，不开放未提交的cache副作用，
遵循生产bulk的提交后发布边界；未声称复现Go测试helper提前evict的时机。
TSO成员清单、批量容量、leadership fence属于42，本项不重复实现或提前标为完成。
GC观察世代/旧publisher栅栏由真实GC拥有任务接入；本项实现提交事件及无锁回调端口。

验证：

- 可运行red 1项真实etcd，成功返回后名称索引仍存在导致断言失败，exit101；实现后通过。
- 7项聚焦在完整包中执行通过，完整keyspace包92项通过、0失败/0忽略。
  覆盖严格状态/两内核保护/未知enum、过滤与defer无副作用、prepared drop、实际外部状态更新导致CAS冲突，
  以及两个并发移除只有一个成功、计数只减一次。真实etcd watch证明name/meta删除与assignment更新三事件同revision。
- 实际准备事务后停止etcd，commit失败；metadata cache和真实TtlCache观察项均保留。
  同WAL重启后fresh SDK确认metadata/计数未丢，再次remove成功清除索引、计数与两个cache。
  observer实际检查metadata mutex已释放、Cache item已删除，随后清除从真实持久GC barrier读取的观察bytes。
  持久GC barrier本身保留，未混淆物理GC数据与观察缓存；测试最后关闭TTL worker。
- 同源既有Go keyspaceGroup suite实际三叶例通过：protected/duplicate过滤与重试、assignment聚合、duplicate不重复扣减。
  原例使用MemoryKV，Rust另补上述真实etcd/并发/提交故障/WAL证据，没有用原MemoryKV替代耐久性。
- 同源GC TestBarrierMetricsRemovalFencesInflightPublication一叶例通过，实际嵌入etcd，验证物理移除清理观察样本，
  已在途GC事务提交也不能恢复样本。此为后续GC owner的对接语义，不冒充Rust的完整metrics世代实现。
- 在线Go现有生产删除入口执行Archived键空间移除，Rust执行owned remove，比较本项metadata/name/counter边界：
  原始物理键/bytes及create/mod revision保存；实际watch确认相关三事件在双方各自同一revision；
  删除后point/name查询同为NotFound，counter为0，双方同WAL恢复后状态不变。
  Go入口额外维护TSO membership，明确归42，未把额外行为从记录中伪装成本项已完成。
- fmt、workspace check通过；sa_002动态metadata守卫1项/5负向控制通过，78包698生产源码。
  完整workspace test按外层任务24统一回归，零doc-tests不计行为证据。
- 每个Cargo阶段动态领取槽位1，绝对CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  fixture/子进程结束后释放自有锁，保留缓存。

Go服务、SDK与官方etcd来源沿用任务001；Go测试二进制与独立只读依赖安装来源沿用39。
1101项仓库Go/模块首尾SHA及存在性一致；未改Go、模块、failpoint、plan.md或来源史诗。继续41保留空间初始化。

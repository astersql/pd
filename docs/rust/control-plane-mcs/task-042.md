# 任务042：批量移除分组键空间

状态：已完成，2026-11-05，基准215c62398。
[task-042-evidence.json.gz](task-042-evidence.json.gz)保存阶段命令、输出、Go原例及在线双方真实事务事件。

`GroupRemovalJob`由宿主逐批驱动或调用run，不创建后台worker/runtime。
遵守Go的120写操作预算、59成员扫描上限、39请求单事务保障；按原始请求长度选择路径，
大请求扫描持久成员顺序，小集合迭代次序不作保证。跳过保留、缺失、Enabled/Disabled及非本组空间；
只删除Archived/Tombstone，去掉被删除成员的重复项，保留其余成员顺序。
每批重新加载实际组和metadata，以revision CAS共同提交名称/metadata删除、按meta服务组聚合的计数递减和组成员更新。
可选Leadership保留value/lease/mod revision完整任期比较，每批校验；部分成功后的错误不会回滚已提交批次。

跨批持有membership读锁，阻止已有成员迁移、建删组及开始split/merge；新建键空间仍能在批间加入，下一批保留它。
接线覆盖30/31/32/38原有写路径，锁顺序固定在组/metadata锁之前。GC观察者只接收确认提交的删除，调用前释放组索引和metadata锁。
删除阶段或提交报错时驱逐已暂存ID的metadata cache，并从独立于取消/旧任期的真实存储读取刷新组索引；刷新失败保留旧组缓存。
若宿主丢弃正在提交的future，RAII驱逐相关cache并终止该job；不启动隐式异步清理，组索引由重试重新加载，未确认结果不调用GC观察者。
准备后尚未开始commit便丢弃的批次不发布副作用。

真实容量测试暴露共享MetaStore预校验错误：此前把compare和写操作数量相加限制128，拒绝合法事务。
只读etcd3.5 server `api/v3rpc/key.go:141–151`检查各分支数量的最大值，现同步修正typed/raw入口。
独立memory及真实etcd测试验证128比较+128写入被接受；任一分支129被拒绝且状态不变。
fixture增加可选真实`--max-txn-ops=120`，默认夹具行为保持。

来源：`pkg/keyspace/tso_keyspace_group.go:952–1417`及相邻单测；生产调用为
`server/apiv2/handlers/tso_keyspace_group.go`的DELETE keyspaces入口。
同时检查集成测试的组API/锁与TSO调用；该套件SetupTest启用failpoint，未裸跑它。
Go现有故障原例中的ambiguous storage包装器实际提交原存储后返回DeadlineExceeded；记录其原始语义，
不将其称为真实网络故障。Rust相应测试没有伪造存储返回值，而在独立真实watch确认提交后丢弃尚未消费的SDK future。
未运行需要EnableCall注入的Go领导权/取消/分批暂停原例；这些故障由真实Rust任期、取消和停止etcd边界验证。

验证：

- 初始可运行red：实际组成员未移除，断言失败exit101；实现后通过。
- 聚焦10项真实etcd通过，keyspace整包107项通过；storage-kv整包34项通过。
  严格120操作etcd下122个大metadata共享绑定分成59/59/4，71个不同绑定分成39/32；
  所有metadata/name和计数最终一致，32KiB配置避免误用值比较放大请求。
- 部分提交后取消保留后续项；准备下一批后停止真实etcd，失败驱逐cache、刷新失败保留组缓存；
  同WAL fresh SDK重试完成，仅Enabled空间及其计数保留。
- 持久metadata解码故障仅驱逐已暂存项；真实并发组revision重写导致整批CAS失败，无局部删除/计数改变，组索引刷新。
- 同值新lease/mod revision替换真实leader key后旧任期后续批次失败；新任期重试完成。
  独立watch确认真实提交、丢弃未消费commit future，暖cache全部安全驱逐，重试不会漏掉已删ID。
- membership读锁阻止并发配置迁移，但真实37创建流程能在批间加入新成员；后续回收保留新增成员，随后迁移完成，计数为2。
  另覆盖保留/重复/缺失/外组成员、取消入场、错store/path作用域、缺组和split/merge拒绝。
- 同源只读Go二进制运行真实etcd容量、无效上下文以及五个suite行为方法通过（含split/merge两个子例）；
  包括死锁回归、批量过滤、提交结果不明后的cache刷新及刷新失败保留。原始test2json和二进制SHA入证据。
- 在线Go API模式真实DELETE，分别122共享绑定及71不同绑定；Rust运行同一输入。
  双方完整最终Group DTO一致，全部metadata/name确实不存在，所有status计数为0。
  watch保留真实物理键与revision，逐批原子事件数共享绑定120/120/10、不同绑定118/97，双方一致。
  小集合map顺序不作固定断言，原始删除键序和revision均保留，不归一化事务边界。
- fmt、metadata、workspace check及sa_002动态架构守卫通过；完整workspace test按外层24统一回归。
  Cargo动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；每阶段结束释放自有锁。
- 1101项Go/模块文件存在性及SHA完全不变；未改Go、模块、failpoint、plan.md或来源史诗。

官方镜像及同源测试构建来源沿用任务27解阻与39；生产Go服务及官方etcd沿用001。
继续43真实GC状态读取。

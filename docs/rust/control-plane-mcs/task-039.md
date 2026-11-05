# 任务039：键空间生命周期

状态：已完成，2026-11-05，基准e5a038930。
[task-039-evidence.json.gz](task-039-evidence.json.gz)保存原始阶段、同源Go测试构建/逐例、RPC/HTTP差分与物理状态。

UpdateStateByID/ByName共享唯一转换实现；0→1、1→0/2、2→3合法，任何相同原始enum值幂等，
包含未知值99。变化时更新state_changed_at；同状态不改时间，但仍执行Go原有metadata保存。
Classic保护DEFAULT/0，NextGen保护SYSTEM/0xFFFFFE；保护检查先于存储读取，错误文本与Go一致。
37的创建启用改为调用这一公开API，移除局部启用实现。

Tombstone删除非空meta-service binding并在同一metadata事务递减计数，已清除则不重复扣减；
旧Tombstone残留binding的相同状态更新仍修复。计数更新不在metadata锁内重入目录锁，避免配置更新锁序反转。
Cache保留Tombstone item供全量遍历，但keyspace_exist为false。按Go原行为，变化后的cache在提交前发布；
真实提交拒绝时持久metadata可仍为Disabled、cache已为Enabled，此事实明确覆盖，不伪称回滚cache。
只读来源keyspace.go:1025–1081、1177–1213、1226–1276以及util.go:54–63、348–363。

GC失效边界：状态变化本身不立即清空GCStateManager本地cache。Go gc_state_manager_test.go:2260–2337
真实检查状态变化后cache仍存在，GetAll(include barriers=true)随后才清除归档项。
该既有测试已真实运行通过。39不创建第二个GC manager或伪造失效回调；43负责真实GC读取/过滤/失效，
40/42负责Go物理移除时的显式回调。本项没有把GC manager或GC RPC装配标为完成。

验证：

- 可运行red 1项真实etcd：状态没有更新导致断言失败，exit101；实现后通过。
- 聚焦6项真实etcd通过；完整keyspace包85项通过、0失败/0忽略。
  固定25组转换含未知enum、两种入口、幂等时间、缺失、两内核保护先于IO、墓碑计数修复、
  Tombstone缓存可见性、真实停服与同WAL恢复。恢复持久化检查使用fresh SDK，保留旧连接Unavailable的初次失败记录。
- 真实官方etcd二分寻找可接受单put的物理大小上限（filler约1572770字节），之后带CAS/时间更新的状态事务
  被实际请求大小限制拒绝；原metadata bytes/mod revision不变，cache的提前发布符合Go源码顺序。
  未用mock、预制错误或固定调用次数代替网络/事务结果。
- 真实Go服务执行37组记录：25组gRPC enum矩阵、两组protected/missing、七组HTTP按名迁移、
  旧墓碑binding/计数修复、真实状态事务拒绝及双端同WAL恢复；另断言DEFAULT名称保护。
  成功领域值/持久protobuf摘要和非法转换消息相等，保留header cluster ID、错误类型、原始物理键及revision。
- Go独立故障输入为恰好2MiB的可读metadata，随后状态事务超过其SDK发送上限而返回HTTP500；
  metadata在同WAL恢复后不变。Go默认嵌入etcd允许150MiB，Rust native fixture默认1.5MiB，
  两个故障使用各自实际边界，未冒充相同字节阈值。Go私有state cache没有RPC读入口，
  提前发布的Go顺序来自只读源码，Rust实际cache结果由上述native故障断言，未声称RPC直接读到了Go私有cache。
- 复用Go1.26.8隔离构建既有keyspace测试的nextgen变体，Classic/NextGen各运行TestValidateID一例通过。
  未运行需要mockNextGenBuildFlag注入的测试。既有pkg/gc测试隔离构建后运行
  TestGCStateManager/TestGetAllKeyspacesGCStatesExcludingGCBarriersFiltersStaleCachedState一叶例通过，真实嵌入etcd。
- GC编译首次发现两个缺失的锁定模块，安装上游原始go-deadlock v0.3.9与goid a731cc31b4fe到独立依赖缓存，
  校验Sum/GoModSum均与只读go.sum一致；来源/摘要/构建环境保存。编译和运行期间仓库、SDK、原缓存及新缓存均由
  sandbox拒绝写入，未注入failpoint或编写Go工具/测试；编译中间产物仅在隔离输出目录。
- fmt、workspace check通过；sa_002动态metadata守卫1项/5负向控制通过，78包697生产源码。
  完整workspace test按外层任务24统一回归，零doc-tests不计行为证据。
- 每个Cargo阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  fixture/子进程退出后释放自有锁，保留缓存。

Go服务与官方etcd来源沿用任务001，固定kvproto f8ad079c8e9812ed5900b0c12b5f1d97171464c4。
1101项仓库Go/模块首尾SHA及存在性一致；没有仓库Go/模块、plan.md或来源史诗修改。继续40物理移除。

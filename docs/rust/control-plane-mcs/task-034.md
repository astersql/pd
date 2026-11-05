# 任务034：分组调和监听与任期保护

状态：已完成，2026-11-05，基准09cc7b07c。
[task-034-evidence.json.gz](task-034-evidence.json.gz)保存全部阶段、失败、原Go逐例、metadata和源码摘要。

GroupReconcile返回宿主驱动的独立节点/分组观察器和任期状态，不分离后台任务、不建私有runtime。
复用ReconciledWatch一致快照、revision续接、网络重连和compaction后缺失键重载；
etcdutil生产依赖改为storage-domain，其原生SDK仅留在测试依赖，避免领域反向依赖适配器。
首次组快照建立完整user-kind缓存，后续监听只维护任期独立的轻量成员/过渡状态索引，
匹配Go外部新增组不立即写入manager完整缓存的语义；实际分配前再读取最新持久组。
前缀解码在id和members就绪后停止，不扫描后面的巨大keyspaces数组。
坏路径/ID不匹配使整个批次不发布、不推进cursor，修复后宿主可显式reload。
节点注册map保留真实服务地址，compaction重载同时删除消失的注册键。

每次调和只选择排序后的非过渡候选。健康索引不会扫描全存储；两种观察器均用当前任期
publication guard，替换任期取消旧观察状态，旧批次不能污染新状态或完整缓存。
分配复用33的一条算法，仅在提交边界通过CallbackTxn::into_parts转交完整读条件及写集合，
由现有Leadership::transact追加value/lease/mod_revision比较，后端资源身份在开始时核验。
相同leader值重新竞选也会拒绝旧任期实际写入。纯取消不冒称已派发事务被回滚；
不可确定结果沿用Unavailable，已取消观察状态不再发布本地结果。

本轮验证：

- 最初可运行red 1项，持久组未进入初始观察索引，退出101；接入快照后通过。
- 本项最终8项聚焦通过，键空间完整包51通过/0失败/0忽略；其中包含真实etcd外部
  创建/更新过渡状态/删除、真实TCP地址注册后分配、compaction缺失键清理及同WAL停服恢复。
  健康组的轻量索引就绪后实际停止存储，调和仍不产生读取或候选；恢复后新组继续分配。
- 任期ABA测试先撤销旧lease，再用同一leader值竞选新lease；旧本地token仍未取消时，
  实际旧任期分配被NotLeader拒绝，目标值及mod_revision保持不变，未发布缓存。
  本轮回执old/new leader revision为4/6，目标组revision为7/7，lease也不同。
  新任期加载并成功分配；旧观察器取消、旧调和拒绝，旧状态未进入新状态。
- 追加真实错误作用域red：带任期保护的超大组提交失败后只尝试了组1，未继续组2，退出101。
  修正ElectionError::Store的分类后，同样实际etcd大小拒绝只记录单组失败，组2仍提交成功。
- 首轮完整包49通过/1启动失败，诊断明确peer端口bind: address already in use。
  仅对首次native启动的该错误最多5次重新申请端口，原数据从未健康启动才清理；
  WAL重启保持原端口/目录，其他错误和业务断言不重试。新增实际持有peer端口克隆的测试，
  官方etcd首次确实绑定失败、下一预约成功并完成真实读写；随后包级51及相邻12通过。
  早先任务28无日志的启动退出仍保留历史不明原因，本项不倒推其根因。
- etcdutil完整12通过，KV callback_txn相邻2通过。workspace check及fmt检查通过；
  sa_002架构守卫1通过、5负向控制、78包691生产源码，完整metadata/源码SHA已保存。
  完整workspace test仍按外层任务24统一回归；零doc-tests不计行为证据。
- 复用只读同源Go测试二进制，TestGroupsNeedingAllocationReturnsSortedIDs、
  TestReconcileKeyspaceGroupsScopesErrors、TestKeyspaceGroupReconcileWatcherTracksExternalChanges、
  TestKeyspaceGroupReconcileStateIsTermLocal、TestDecodeKeyspaceGroupReconcileEntry五例实际通过。
  外部变更Go用例使用真实嵌入etcd，其余按原fixture运行；Rust回放原ID1/2、成员地址、
  键空间41/42/43、ID7前缀和1/2/3排序输入，并追加实际压缩/重启/任期写入保护。
- 每阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  所有验证、监听和etcd进程退出后释放自有锁，缓存保留。

Go/模块1101项首尾SHA与存在性一致；Go构建输入、SDK及module cache未改写，未注入failpoint。
所有plan.md和来源史诗只读。角色监督器持续驱动这些pull API的装配由131负责，
自动大组拆分为下一拥有任务35；未将这些后续行为计入本项。

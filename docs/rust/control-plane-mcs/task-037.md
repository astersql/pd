# 任务037：创建并查询键空间

状态：已完成，2026-11-05，基准29c5f0448。
[task-037-evidence.json.gz](task-037-evidence.json.gz)保存全部阶段、运行差分、物理状态、metadata和Go输入摘要。

KeyspaceManager复用宿主MetaStore、25的校验、26的Cache、27的分配预留、29的目录、
36的真实labeler/边界等待和既有带任期保护的Allocator；没有第二个ID分配器或proto领域类型。
Keyspace/Identity及存储算法从endpoint移至storage-domain，KeyspaceStorage接收宿主codec；
原endpoint只保留pinned protobuf转换并薄委托旧API，公开值类型仍为同一身份。
点读的proto-unmarshal错误与range decode错误保持区分，编解码异常通过中性错误边界传递。

只读来源为keyspace.go:275–650（Create/ByID/saveNew/分组），770–814（Load），
1279–1308（range），1447–1458（allocID），tso_keyspace_group.go:897–930及1419–1479。
名字验证和存在性检查在分配ID前；ByID按Go单独检查缺省ID/空名，并保留其不调用allocID校验的行为。
配置清除响应端点、伪造meta group和TSO group；启用group manager时选择最少成员组并覆盖kind/bound，
NextGen为空gc_management_type补keyspace_level；响应装饰端点不写回存储。
Go heap未规定同数量组的次序，Rust在最少数量组内按ID确定次序；不宣称Go提供稳定tie次序。

创建按Go分阶段运行：原子提交名称和Disabled元数据、发布cache、注册规则并等待、启用元数据、加入TSO组。
这里的原子性指名称/ID索引，不把Go尚未原子的整个流程伪称单事务。
meta分配复用27的同事务预留并保持目录锁到提交，失败不留下计数。
split失败时尝试同事务删除索引和撤销分配，主错误及可选cleanup错误分别保留。
像Go一样，此路径不移除先前Disabled cache项；最终组加入失败也保留已经Enabled的元数据和规则。
创建等待期间不持有元数据锁，其他读取能看到Disabled配置；动态wait开关在保存后读取。
查询使用真实revision读事务、名称→ID→meta检查、inclusive起点与0无限制范围，拒绝超过uint24的scan起点。

本轮验证：

- 运行red 1项，名称索引缺失导致真实状态断言失败，exit101；补实现后通过。
- 聚焦9项真实etcd通过；完整keyspace包74项通过、0失败/0忽略。
  包含独立manager并发重名/同ID冲突、create_revision相同的索引对、分页/孤儿/损坏值、
  NextGen/Classic及配置覆盖、分配/端点装饰、超时索引与计数回滚、实际请求过大拒绝、停服读取失败、WAL恢复。
- 复用真实Member/Leadership和Keyspace namespace Allocator；重复名不消耗ID、下次仍为2。
  wait过程中实际读取Disabled，向共享Core索引加入边界后转Enabled；等待期间组被删除时，
  最终返回精确NotExists且已经Enabled的metadata和规则保留。
- 元数据中性化后原storage-endpoint keyspace_过滤18项相邻通过，包括真实etcd与旧格式/错误断言。
- 两项显式--ignored在线Go差分实际执行并通过（未将ignored当成成功）：
  正常模式验证两次自动分配及重复名、三次指定ID创建、同名/同ID冲突、三种非法输入、名称/ID查询、
  分页及真实双端同WAL恢复。以实际Go创建时间作为共享输入，指定ID的元数据原始protobuf字节完全相同。
  保存原始名称/ID键、bytes、create/mod revision，双端均证明索引同次创建、启用只推进meta revision。
  API模式验证unknown kind→basic、强制TSO组0、txn bound及伪造端点清除，完整DTO和组元数据相等；
  多项config的protobuf map编码次序不作稳定承诺，原始字节保留，pinned codec解码后领域值完全相等。
- Go keyspace主suite显式启用skipSplitRegion，未裸跑或注入；使用准入真实服务现有HTTP创建入口验证。
- fmt、workspace check通过；sa_002动态metadata守卫1项/5负向控制通过，78包695生产源码。
  完整workspace test按外层任务24统一回归，零doc-tests不计行为证据。
- 各阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  fixture/子进程结束后释放自有锁，保留编译缓存。

准入Go server及SDK、官方etcd沿用任务001，Go server SHA256为
364477a06e13afb29626ba57dbbd0be8245d0e9d68ad30f0dbeeda3dce3a3147，固定kvproto
f8ad079c8e9812ed5900b0c12b5f1d97171464c4。1101项Go/模块首尾SHA与存在性一致，
没有新增或改写Go、模块文件、failpoint或plan.md。宿主完整装配仍归131；GC验收尚未由本项完成。

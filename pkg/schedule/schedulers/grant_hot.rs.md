# grant_hot.rs

任务96的canonical实现。`GrantHotConfig`保留store-id顺序、重复元素及setStore自动
补leader行为。CLI严格ParseUint十进制；HTTP适配输入拒绝数值ID。`GrantHotScheduler`
提供schedule/dispatch/transfer、独立region与leader限额判断、encode/persist/update/reload。
MetaStore通过真实StorageEndpoint接入，更新保存失败保留新store列表并将leader清零；
reload先读取并完整解码，失败不改变内存。HTTP路由、统计快照刷新和operator提交由宿主装配。

schedule接受四种HotResource的真实StoreLoadDetail快照，随机资源、分支与目标分别由
宿主提供随机字。dispatch按当前byte负载降序，保留热peer顺序，跳过活跃operator；
失败继续搜索，最多一个真实操作器。Go中的isLeader分支确实跳过配置内的源，peer
分支只选配置内非固定leader源；此处忠实保留。相同负载Go无稳定顺序，本实现保留输入
顺序，测试只用不相等负载，不声称map迭代顺序等价。

transfer拒绝缺失/有down或pending的region、缺失源；复用PlacementGuardContext的
普通safeguard（leader分支也与Go一致）、HIGH StoreStateFilter、真实leader-in预算，
peer分支再排除已有副本和HotRegionEvictedTargetFilter。目标只来自配置。复用
PeerOperatorFactory构建LEADER或REGION|LEADER操作器并设HIGH，无额外HOT_REGION位。
分支选择与调用者给定统计资源类型独立；schedule限额检查是coordinator独立入口。

来源：只读grant_hot_region.go、grant_hot_region_test.go及init.go。按用户授权采用
源码/测试推导与Rust真实行为验证，没有Go运行oracle或运行差分。指定Go测试分别验证
非字符串leader-ID拒绝与leader-in预算耗尽/解除。本任务Rust覆盖两者，并增加配置
错误、存储读写故障、损坏数据和allocator故障，详见
[证据](../../../docs/rust/schedule-execution/task-096.md)。无新增feature、runtime或proto依赖。

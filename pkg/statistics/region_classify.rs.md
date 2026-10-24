# 区域异常分类索引

`region_classify.rs` 在 canonical statistics crate 内提供 `RegionStatistics`、
`RegionStatisticType`、`RegionInfoProvider`、`RegionAnomalyDurations` 和
`PlacementRulesRequired`。证据见 [任务15](../../docs/rust/state/task-15.md)。

`observe(region, stores, conf, now_unix_seconds)` 复用 core 的真实 RegionInfo、StoreInfo
和 schedule/config 的 CheckerConfigProvider，每次调用读取同一代配置快照。
此入口覆盖 placement 禁用分支，启用返回显式错误且不改变任何索引。
启用placement请使用[规则入口](region_placement.rs.md)，复用同一内部分类路径。
10种单bit分类与Go顺序相同：miss/extra/down/pending/offline/learner/empty/oversized/undersized/witness。
offline仅考虑region确有peer的Removing store；learner计入总peer数，witness看leader，nil安全。

索引保存 ID→bit及分类→ID集合。observe替换旧状态、移除已不成立分类；
`clear_defunct_region`同时移除时间戳及零bit的已观察状态，重复清除安全。
`region_ids_by_type`返回有序ID；`get_region_stats_by_type`读取宿主当前owned快照并跳过已删除ID，
不返回历史RegionInfo引用。宿主负责缓存及并发保护，库不持有全局锁、runtime或存储。

`region_stats_need_update`保留Go的触发范围：未观察或down/pending/learner/empty/oversized/undersized变化。
副本数量、offline、witness由observe更新，不额外扩张此触发函数；配置改变大小/keys/merge阈值会重新判定。
size单位MiB；大小或keys达到上限即oversized，两维都不超过merge阈值为undersized；empty需size<=1且keys=0。

miss-voter/down持续时间以调用方传入Unix秒累计；首次仅记开始，后续返回从开始时刻的采样，
解除分类或clear后重新计时。保留Go的0哨兵及回拨时的有符号秒差。
宿主消费采样记录histogram，库不安装全局Registry；本模块不负责日志与指标注册。

独立 `region_classify_test.rs` 在lib显式登记，包含Go原事件序列及扩展阈值、配置、
清除、当前快照、witness/nil leader、时钟及不支持placement边界测试；不mock分类结果。

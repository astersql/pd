# 标签隔离状态统计

[实现](label_statistics.rs)提供`get_region_label_isolation`、
`is_region_label_isolation_satisfied`和`LabelStatistics`，
对齐Go region_collection.go的同步内存行为，默认features且无新增依赖。

隔离层级是使每对store至少有一个已知且不同标签的最浅前缀。
调用canonical core的StoreInfo::compare_location，复用标签key和值的Unicode简单折叠；
缺失及空值匹配任何值。逐对取首次差异的最大层级等价于Go逐层细分未隔离组，
避免缺失值带来的分组复制。时间O(stores² × labels)，辅助空间O(1)。
空stores或空labels、存在始终无法区分的pair返回("none", -1)；单store返回首层。
返回的label保留配置原始拼写；要求值空或"none"总满足，其他要求按原始key精确匹配，
所需层级不能高于实际隔离级别，未知key不满足。

LabelStatistics使用真实RegionInfo的ID保存所属隔离级别；同级重复observe无变化，
改变级别先减旧再加新，零计数项保留。mark_defunct_region只延迟标记，
重复mark去重；clear_defunct_regions按清理时的当前级别减一次并删除region，
未知ID跳过但标记同样清空；mark后observe不会取消标记。
label_counter返回独立有序快照，供宿主管理的指标收集器使用；不注册全局指标或后台任务。
宿主用独占可变引用（需要共享时自行加锁）串行化写入。

[独立测试](label_statistics_test.rs)在lib.rs显式注册；
[验收证据](../../docs/rust/state/task-17.md)区分真实Go运行oracle与源码契约扩展。

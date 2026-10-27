# 调度热点查询与观测

`Coordinator::get_hot_regions_by_type`借用宿主当前`HotObservationInputs`，调用canonical
`statistics::get_hot_status`；输入peer map应是宿主`GetHotPeerStats`等价的当前过滤快照，
loads使用`StoreKindLoads`原单位。可从`StatisticsSnapshot::get_hot_peer_stats`的BTreeMap
收集为HashMap；门面不拥有cache、不缓存查询结果、不复制operator或statistics状态。
`is_trace_region_flow`使用现有配置provider，聚合的引擎/方向/正hot-degree规则仍在statistics。

门面从同一BasicCluster查当前region/store：两个人口的learner保留Go现有
`region.GetPeer(h.StoreID)`行为，即把hot项store ID当作peer ID查找（非GetStorePeer）；
write更新时间使用region报告结束秒，read使用store最后心跳纳秒。缺region保留默认learner
和write零时间，read仍按store更新时间。UTC Gregorian格式保留Go RFC3339Nano的尾零裁剪，
负纳秒正确向下取整，region u64秒按Go int64转换；没有墙钟读取；仅将同一Unix instant统一表示为UTC，不舍入精度。

`get_hot_regions`空IDs返回全部；显式IDs保留未知store的null，重复ID不覆盖已有值。
`HotRegionsQuery`只在此门面使用Option表达Go指针map，避免修改canonical统计聚合类型。

`HotSpotMetrics::register`注册到宿主Registry；返回Prometheus错误，不安装全局registry。
名称`pd_hotspot_status`、help、gauge类型和address/store/type标签保留Go定义。
首次创建series时缓存Gauge，周期收集复用；clone共享handles、reset和删除。
`Coordinator::collect_hot_spot_metrics`给出当前store snapshot；底层同名方法允许宿主
显式传入snapshot，发布后重查canonical store，防止旧snapshot复活removed/deleted store。
已有removed snapshot直接走各series删除分支，不进行全vector扫描。

metrics的轻量totals调用`collect_hot_peer_infos`，包含全部输入peer（不同于query正degree
过滤），覆盖byte/key/query/cpu和count的read/write、leader/peer。缺map键删除series，
存在空人口发布0。任一RW无leader/peer人口时，按Go删除该store全部RW pending维度。
`delete_store_metrics`只删hotspot；`reset_hot_spot_metrics`清空hotspot及共享scheduler的
HotPendingSum，保留其它scheduler families；清除handles后重新收集可见新series。
必要的scheduler窄接口`delete_hot_pending_metrics`同时维护其vector和series集合。

pending权重和operator状态继续由`HotLoadProjection`管理，门面query不叠加pending。
真实operator的running、cancel、success及zombie窗口通过既有pending汇总和
`SchedulerMetrics::collect_hot_pending_metrics`观察，未建立第二份计算。

[任务证据](../../docs/rust/schedule-execution/task-116.md)记录来源源码SHA、独立fixture、
行为红灯和通过日志。本任务使用用户授权的源码推导期望及真实Rust验证，未运行Go oracle。

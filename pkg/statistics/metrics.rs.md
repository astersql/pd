# 统计指标宿主注册与清理

[metrics.rs](metrics.rs)提供`StatisticsMetrics::register(&Registry)`，仅使用workspace固定的
单一Prometheus版本。9个GaugeVec和1个HistogramVec的name/help/type/label顺序来自
[metrics.go](metrics.go)，无全局注册器、推送、运行时或协议依赖。metricutil当前Rust仅有
MetricConfig，无注册/标签工具；本实现复用既有Prometheus依赖，不复制metricutil业务。
注册失败回滚本次已注册的collector，不卸载宿主原有collector，返回Prometheus错误。
注册完成保留10个region零值与2个异常时长histogram；histogram用秒、30个1*1.4^i桶。

`set_gauge`校验family/label数量；缓存标签对应Gauge，采集时复用句柄。删除与设置共享
互斥锁，删除同时丢弃缓存，后续采集重新取得附着于Registry的序列。宿主串行安排整个
collect/reset生命周期，单样本并发安全不意味着整个采集批次原子；注册/卸载由宿主管理。
`collect_store_statistics`消费既有StoreStatisticsMap样本，先重建placement census，
其他family保留Go最后值。`observe_hot_stat`读取真实StoresStats快照，输出12项平滑速率
与9项instant速率，跳过removed及缺失历史，避免在途心跳重建历史后重新发布墓碑store。
`collect_region_statistics`读取真实10类region index，`collect_label_statistics`读取标签计数；
`observe_region_durations`消费真实观察返回的累计秒数，不重复实现分类计算。

`reset_store_statistics(id)`按store清除所有旧地址及任意placement name，精确删除tikv/
tiflash六项资源与add-peer/remove-peer/transfer-leader-in三项limit；保留12项生命周期。
`delete_cluster_status_metrics`仅删除指定store/engine的12项状态，资源与另一个engine保留。
`reset_region_stats_metrics`把10个缓存序列置零；label/hot reset删除序列。
`reset`删除store/placement/cluster/limit/label/hot并置零region；config、hot summary、
histogram保留，遵守Go Reset。数据源历史/配置不由指标清理函数改变。

`HotPeerCache::collect_metrics(now, active)`生成独立`HotMetricsSnapshot`，包括真实TopN
阈值、计算阈值时的total_length及Add/Remove/Update次数（Gauge）。store标签为store-N，
方向为read/write。先采样再执行节流GC，删除已移除store在本次采集产生的序列；
threshold-only、action-only、无新流量、显式gc后再采集均按当前缓存清理旧序列。
GC直接过期删除不增加report Remove计数，重新激活store从新计数开始。
`StatisticsMetrics::collect_hot_cache_metrics`按完整方向快照删除缺失label，再写入当前样本；
不依赖一次性通知，丢弃已完成的采集结果后，下次采集仍清除旧标签。
HotCache通过既有FIFO `HotTask::Metrics`在两个worker驱动采集/GC，不另建后台任务；
宿主仍须cancel/shutdown并join。两方向采集不承诺原子性；错误保留既有队列错误。

[独立行为测试](metrics_test.rs)由[lib.rs](lib.rs)显式登记，
[证据](../../docs/rust/state/task-28.md)包含Go快照、真实失败断言、回归、只读检查和槽位退出。

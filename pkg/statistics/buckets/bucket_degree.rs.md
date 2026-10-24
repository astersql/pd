# 桶冷热判定与查询

来源：相邻 `bucket_stat_informer.go::calculateHotDegree` 与
`hot_bucket_cache.go::GetHotBucketStats`。实现与运行证据见
[任务7](../../../docs/rust/state/task-07.md)。

`BucketTreeItem::calculate_hot_degree` 只修改各桶的hot_degree。
读byte/key、写byte/key四个rate任一严格大于canonical statistics-utils阈值即热；
QPS不参与。热且degree<20时+1，冷且degree>-20时-1，不在切换时重置。
正常报告从0经继承后演化至[-20,20]；对外部赋予的越界值保持Go条件步进，
不进行额外clamp，避免改变可观察行为。

`BucketRangeIndex::get_hot_bucket_stats` 从当前region映射筛选degree>=min的桶，
空ID列表代表全部，重复ID只返回一份，忽略未知ID和没有匹配桶的region。
只读内部iterator让本模块访问current regions，不暴露可变索引。
返回HashMap的region顺序未定义，region内桶顺序保留；键、负载及其余字段均深复制，
旧查询快照不随后续报告变化，调用者修改结果不会修改内部状态。
归档碎片保留在范围树供继承使用，但不进入此查询。

报告顺序由调用方组合convert→inherit→calculate→put，后续工作队列复用这些领域步骤。
本模块无异步任务、时钟、I/O、协议类型或存储依赖。

# 桶范围覆盖与碎片

Go来源：hot_bucket_cache.go 的 bucketDebrisFactory、putItem、getBucketsByKeyRange，
bucket_stat_informer.go 的 cloneBucketItemByRange；验收见
[任务5证据](../../../docs/rust/state/task-05.md)。

BucketTreeItem 实现 canonical RangeItem；BucketRangeIndex 组合 RangeTree 与当前
region报告索引。put_item独占借用内计算重叠、替换范围并更新region索引，不接受易过期的
外部overlaps参数。重叠Alive项移出region索引，Archive只留树中供后续继承。
同region同范围替换所有字段并同步树视图，保留分片布局。版本不参与范围相等判断。
查询按起始键排序，半开范围，空结束键表示无穷；返回自有快照，跨线程同步由宿主负责。

工厂复用keyutil普通字节min/max并显式处理范围空结束键。左右碎片继承region、version、
interval及各桶热度/负载，status为Archive；桶边界和负载深复制，防止旧快照被更新污染。
保留Go的裁剪细节：stat裁剪不是min_end_key，left<right才保留，空stat.end可能被过滤。
空/空无效工厂碎片由既有RangeTree过滤，不制造额外全范围条目。
本模块只处理范围状态，不判断冷热、不做协议转换、不启动队列或后台任务。

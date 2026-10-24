# 统计容器组合与只读快照

HotStat对应Go hot_stat.go中的HotCache、StoresStats、HotBucketCache组合；
Rust复用canonical buckets crate的BucketWorker（当前公开API名称），并持有SlowStat。
宿主提供Tokio Handle、Registry和hot队列容量，库不创建runtime、调度器、全局状态或协议转换。
构造时先验证容量，再注册bucket指标和启动bucket worker，最后启动hot read/write worker；
Registry冲突在启动任何新worker之前返回，既有容器仍能使用。

stores_stats、hot_cache、slow_stat公开以复用现有报告API；bucket_client返回独立producer。
StoreStatInformer直接转发StoresStats；异步HotCache不能实现现有同步RegionStatInformer，
因此snapshot构造owned StatisticsSnapshot，它实现两个同步informer并提供get_hot_peer_stats、
is_region_hot。SlowStat仍保持独立缓存语义；snapshot只记录其公开exists结果。

snapshot先在read/write队列各入一个Snapshot屏障，再入bucket collect屏障，随后等待全部结果。
报告成功入队且在屏障之前的状态可见；store loads和slow非空值在本次调用内分别锁定复制。
同一队列保持FIFO；并发报告时没有跨组件事务一致性保证，宿主需要静止报告才能取得同一逻辑时点。
Full/Closed/Canceled/WorkerStopped及bucket提交/等待错误保留具体来源，任一失败不返回部分快照。
丢弃查询future不撤回已接受的只读任务；one-shot结果丢弃不会阻塞worker。

HotTask::Snapshot复制全部peer（包括cold/anti未满），保留RegionStatInformer的直接查询语义；
HotPeerStat::clone冻结去噪/舍入的四维每秒速率，无live rolling窗口；store为StoreLoadKind数组，
bucket保持原毫秒输入/秒输出及owned key/load值。快照查询只返回独立值，可由调用方修改其本地副本。
get_hot_peer_stats执行degree>=min、非cold、anti等于该方向默认值筛选；is_region_hot仅检查
传入region成员与degree。bucket min_degree/region_ids直接交给真实bucket缓存。

cancel同时关闭hot admission并取消bucket自己的token，不涉及宿主runtime。shutdown即使一方失败
也等待hot双worker和bucket worker退出；重复调用安全。等待future被取消后，HotCache与BucketWorker
都保留未完成join handle，重试可完成join。Drop只请求取消，宿主必须显式shutdown取得退出保证。

无持久化或watch，不引入MetaStore依赖；workspace回归仍包含MemoryStore/EtcdMetaStore真实契约。
[测试](hot_stat_test.rs)显式注册，覆盖报告、隔离、查询、错误、丢弃future、重复关闭及中断后join；
[任务29证据](../../docs/rust/state/task-29.md)保存Go来源、红绿结果和共享槽位验证。

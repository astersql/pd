# 热点过期与冷副本清理

[hot_expire.rs](hot_expire.rs)扩展canonical HotPeerCache，同步领域内存策略。
check_cold_peer按store过滤缺失region，产生零瞬时loads/in_cold动作；未满窗口保留degree/anti，
满窗口强制各减1，anti<=0移除。复用DimStat/TimeMedian，不模拟冷热结果；
Go冷路径阈值delta前四项布局照原样保留。检查复制真实pending窗口，丢弃动作不推进缓存。
报告间隔为秒、时钟/TTL为纳秒。短于3秒默认忽略，set_denoising(false)可取消过滤。

update_stat_at在应用Add/Update时记录最后更新时间，Remove同时清除；unknown不改变时间。
旧update_stat使用原点0，实时宿主必须一致使用update_stat_at并在报告后显式调用gc。
TTL默认3倍方向报告间隔；set_peer_ttl改变TTL/GC间隔，不改写历史时间。
gc按TTL节流，elapsed>=TTL执行，更新时间+TTL<=now过期；时间回退延后清理。
时差用i128避免i64溢出。宿主传入完整存活store集合（不含removed/tombstone），
GC删除缺失store的peer/TopN/阈值快照，包含仅查询过阈值而未插入peer的store。
未删除store保留阈值刷新缓存，与Go的8秒策略一致。

remove_region幂等清除整个region的peer、TTL和TopN；collect_expired_items继续处理副本集变化。
stores_of_region和regions_of_store从唯一canonical peer索引返回独立集合，不保留空反向索引。
因此一个store的peer过期不会隐藏另一store尚有效的peer；Go GC在任一副本过期时删除整个
region的辅助map，Rust不复制该内部map失配，成员查询始终与实际缓存一致。
这项非均匀TTL边界是Rust一致性验证，不声称已有Go逐事件运行快照覆盖。

库不创建后台任务、全局时钟、运行时、持久化或调度决策；宿主串行调用、提供时钟与拓扑。
[独立测试](hot_expire_test.rs)由[lib.rs](lib.rs)显式注册，
[验收证据](../../docs/rust/state/task-22.md)记录原Go测试、红绿灯及workspace回归。

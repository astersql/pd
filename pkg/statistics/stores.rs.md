# StoresStats 生命周期

对应 Go store.go 的 StoresStats 与 store_stat_informer.go。
复用 [RollingStoreStats](store_rolling.rs.md) 和 core StoresInfo/健康谓词。
内部单个 RwLock 持有所有历史，观测、创建、删除、过滤与快照各自同步。
没有后台任务、私有运行时、全局注册器、协议或存储后端依赖。

new/default 是空集；observe/set 和 region 输入按 ID 惰性创建。
get_or_create_rolling_store_stats 与 get_rolling_store_stats 返回深拷贝；
调用者修改这些副本不会改变缓存。删除可重复；删除后旧副本仍有效，
重新创建从空历史开始。get_store_read_cpu_recent_max 对缺失项返回零。
StoreStatInformer 仅暴露 get_stores_loads：新建 HashMap 和12维值数组，
没有可变缓存引用。维度顺序及单位沿用 statistics-utils StoreLoadKind。

filter_unhealthy_store 接受 canonical StoresInfo（BasicCluster.stores）和宿主
Unix 纳秒时钟，删除缺失、Removed、不健康或物理损毁项。core 不健康阈值为
严格大于600秒；Removing 但未损毁且健康的节点保留。健康恢复不自动重建历史，
下一次观测或显式创建才重建。读取store注册表时持有统计写锁，无回调或反向锁。

region 批量输入用 `(store_id, write_bytes_per_second, write_keys_per_second)`
元组切片替代 Go 三个平行数组，避免长度错配；保持顺序（重复ID依次采样）。
各独立调用为同步原子操作，cluster注册表不是跨多个节点的事务快照，语义同Go。

独立 stores_test.rs 经 lib.rs 显式注册。行为与红绿证据见
[任务11](../../docs/rust/state/task-11.md)。

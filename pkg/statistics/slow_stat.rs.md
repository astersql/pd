# 慢节点集合状态

对应只读slow_stat.go：SlowStoresStats用RwLock<HashSet<u64>>保存成员，
Default/new创建空集合。observe_slow_store_status在同一写锁内检查与更新：
不存在且is_slow=true时插入；其余情况删除，包括重复true。
remove_slow_store_status重复删除安全；exists_slow_stores在读锁内判断非空。
ID 0和u64::MAX不特殊处理，不增加排序、时间、存储或协议依赖。

SlowStat持有公开slow_stores_stats并Deref到同一实例，对应Go嵌入字段的方法提升。
方法接受&self，可由宿主Arc共享；锁不泄漏到API，所有操作线性化。
没有后台任务、私有runtime、全局状态或持久化；锁中不执行调用方代码。

独立slow_stat_test.rs在lib.rs显式登记，覆盖真实Go事件重放、重复true、
false/重复移除、跨store/容器隔离、同store并发观察奇偶性。
[验收证据](../../docs/rust/state/task-26.md)。

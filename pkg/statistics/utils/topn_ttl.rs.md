# 显式 TTL TopN

`TtlTopN<T: TopNItem>` 组合既有 [TopN](topn.rs.md)，通过 `items()`
公开只读查询，通过 `put(item, now)`、`remove(id)` 和 `remove_expired(now)`
维护所有维度与期限一致性。调用方以同一单调时钟原点传入 `Duration`；
没有私有时钟、定时线程、运行时或协议/存储依赖，跨线程同步由宿主负责。

TTL固定，单位纳秒；零TTL在插入时刻到期。Rust Duration不表示负TTL。
到期边界为 deadline <= now；Put和读操作不自动清理，到期但尚存ID更新仍返回true。
续期移除旧期限；显式删除也清除期限，所以旧记录不能误删新版本。
期限溢出在任何修改前panic。两个索引各只保留每ID一个期限，反复续期不累积陈旧记录。
有序期限索引提供O(log M)更新，清理E项为O(E*k*log M)，空间O(M)。

清理复用真实TopN删除，余集自动晋升。返回ID按维度数重复以保留Go
`maintain`可观察契约；相同期限ID顺序不承诺。Arc旧快照不受续期/清理影响。
独立 `topn_ttl_test.rs` 在lib显式登记；运行oracle、红绿及回归证据见
[task-03.md](../../../docs/rust/state/task-03.md)。

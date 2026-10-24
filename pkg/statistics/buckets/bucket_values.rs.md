# 桶报告值与快照

`BucketReport`/`BucketFlows` 是自有输入，不依赖 metapb。每相邻两键对应一个桶，
六维计数顺序为读 bytes/keys/qps、写 bytes/keys/qps。`convert_to_bucket_tree_item`
把毫秒周期换算为每秒整数速率：先用 u64 回绕乘1000，再除周期；周期0使用10000。
空末键原样保留为无穷；不补排序、空流量或长度校验规则。无键及短流量数组panic，
单键产生零桶，多余流量忽略，与Go已有索引边界一致。

`BucketTreeItem`/`BucketStat` 持有键、负载、region、version、周期与初始热度0、Alive。
快照与输入不共享可变存储，Clone递归复制全部数据。Go stat.clone仅复制loads、
键是共享的只读切片；Rust所有权接口额外隔离键修改。范围碎片/热度继承/缓存任务另行实现。

来源：同目录 hot_bucket_cache.go 的 convertToBucketTreeItem、bucket_stat_informer.go
的 BucketStat/BucketTreeItem。运行/源码证据区别见
[任务4](../../../docs/rust/state/task-04.md)。

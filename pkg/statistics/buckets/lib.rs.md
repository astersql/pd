# 热点桶领域 crate

`astersql-pd-statistics-buckets` 是唯一 canonical crate。入口显式登记 bucket_values、
bucket_range、bucket_inherit、bucket_degree 及各自独立的 *_test.rs 测试；
重导出自有领域值、归一化函数、范围索引和冷热边界常量。
生产仅依赖 canonical rangetree/keyutil/statistics-utils，没有协议、网络、存储、
全局状态或运行时依赖。报告值、范围碎片、继承和冷热判定/查询已接入；队列留给后续批次。

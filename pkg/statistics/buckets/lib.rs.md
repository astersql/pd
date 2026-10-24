# 热点桶领域 crate

`astersql-pd-statistics-buckets` 是唯一 canonical crate。入口显式登记 bucket_values、
bucket_range 及各自独立的 *_test.rs 测试，重导出自有领域值、归一化函数和范围索引。
生产只依赖canonical rangetree/keyutil，没有协议、网络、存储、全局状态或运行时依赖。
报告值对象和范围碎片已接入；热度状态、继承计算和队列留给后续批次。

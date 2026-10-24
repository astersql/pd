# 热点桶领域 crate

`astersql-pd-statistics-buckets` 是唯一 canonical crate。入口显式登记 bucket_values
及独立 bucket_values_test.rs 测试，仅重导出自有领域值和归一化函数。
生产无协议、网络、存储、全局状态或运行时依赖。当前只完成报告值对象；
范围索引、热度状态和队列留给后续批次。

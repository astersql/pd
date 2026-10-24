# 热点桶领域 crate

`astersql-pd-statistics-buckets` 是唯一 canonical crate。入口显式登记 bucket_values、
bucket_range、bucket_inherit、bucket_degree、bucket_worker及各自独立的 *_test.rs；
重导出自有领域值、归一化函数、范围索引、冷热边界常量和worker生命周期API。
复用canonical rangetree/keyutil/statistics-utils；异步队列使用workspace锁定的Tokio、
tokio-util、prometheus与thiserror。宿主传入runtime handle、取消令牌和Registry，
领域不依赖协议、网络、存储，不安装全局运行时或指标。
报告值、范围碎片、继承、冷热判定/查询与串行队列已接入；生命周期细节见
[bucket_worker.rs.md](bucket_worker.rs.md)。

# Statistics canonical crate

`astersql-pd-statistics` 是统计包唯一 canonical crate，默认功能集合，禁止 unsafe。
当前公开[CPU归一化](cpu.rs.md)的 store_read_cpu_usage 与 region_read_cpu_usage，
依赖 canonical core 领域值，不依赖 protobuf、传输或调度决策。
`cpu_test` 在 cfg(test) 下显式登记，生产测试位于独立 cpu_test.rs。
后续统计行为在本 crate 原位扩展，不创建平行实现。

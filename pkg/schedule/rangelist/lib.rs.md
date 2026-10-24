# rangelist crate

零依赖 canonical 调度基础 crate；公开 Builder 和 List 领域类型。
私有 builder 实现分段构建，event_sort 保留 Go 1.26 同键排列；
range_build_test.rs 在 lib.rs 用 cfg(test) 和显式 path 登记。
range_list 实现查询与分裂点，range_queries_test.rs 显式登记；领域边界不依赖 protobuf 或具体存储后端。

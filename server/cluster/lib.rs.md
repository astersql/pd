# cluster 领域入口

[lib.rs](lib.rs) 注册 canonical `astersql-pd-cluster` 的心跳状态模块和独立测试模块。
公开 `handle_store_heartbeat` 与 `StoreNotFound`，复用 core、statistics 和 statistics-utils。
领域接口只接受自有类型与宿主时钟；不引入协议、存储后端或调度决策。
目前只包含已有 store 的状态更新，不表示整个 RaftCluster 或服务迁移已完成。

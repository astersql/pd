# cluster 领域入口

[lib.rs](lib.rs) 注册 canonical `astersql-pd-cluster` 的心跳状态模块和独立测试模块。
公开 `handle_store_heartbeat` 与 `StoreNotFound`，复用 core、statistics 和 statistics-utils。
领域接口只接受自有类型与宿主时钟；不引入协议、存储后端或调度决策。
包含已有 store 和 region 的心跳状态更新，不表示整个 RaftCluster 或服务迁移已完成。

[removed_store.rs](removed_store.rs)在真实store注册表锁内复查移除状态并提交统计副作用，
独立removed_store_test测试覆盖检查后移除/删除和共享raw holder保护。

[region_heartbeat.rs](region_heartbeat.rs)公开`prepare_region_heartbeat`、
`PreparedRegionHeartbeat::commit`和顺序入口`handle_region_heartbeat`，返回
`RegionHeartbeatResult`供宿主接线副作用；以真实core完成继承、guide与提交前重判。
独立region_heartbeat_test覆盖重复、乱序、并发合并和根树/子树状态。

`handle_region_heartbeat_with_overlap_cleanup` composes core admission with optional
anomaly cleanup and deferred label marking for actual evicted IDs. Same-ID
survivors and rejected reports retain their statistics. Hot history uses its
existing expiration lifecycle; see [region_overlap.rs.md](region_overlap.rs.md).

`StatePipeline` and `StateSnapshot` compose the existing state owners and expose
owned query views; state_pipeline_test is explicitly registered. The pipeline
feeds canonical hot flow for accepted reports, including duplicates, and shares
Downstreams with the optional server RegionSyncer.

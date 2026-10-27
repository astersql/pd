# Cluster utilities crate

`astersql-pd-cluster-utils` 是 `pkg/cluster` 的 canonical Rust crate。
只导出 [async_stats.rs](async_stats.rs.md) 的事件桥接与独立准入结果，
领域依赖限于 core/statistics，不反向依赖 server/runtime。
真实调度器等只作为 dev-dependencies 用于消费验证。

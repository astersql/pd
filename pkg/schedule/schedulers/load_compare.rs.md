# 调度负载比较与待定权重

`load_compare.rs` 是 canonical schedulers crate 中的领域工具，复用 statistics 的
`StoreLoad` / `StoreLoadPred` / `Influence` 和真实 `Operator`，不依赖 protobuf、
传输层或私有运行时。

`rank_cmp` 将 caller 提供的离散 rank 按 -1/0/1 比较；`st_ld_rank_cmp` 投影任意
rate 或 hot peer count。负数分箱由 rank 决定，源码案例用向零截断，不能改成 floor。
`slice_load_cmp` / `slice_lp_cmp` 在第一个非零结果处短路，空列表返回 0；
`neg_load_cmp` 反转结果。预测比较使用现有逐维 min/max/diff，diff 包括 peer count，
没有另建预测类型或复制统计算法。

`PendingInfluence` 持有 Arc<Operator>，保存源 ID、目标 ID、原始 Influence 和
非负 Duration 窗口。`weight(clock)` / `calc_pending_influence` 先检查 expiry/timeout，
随后读取真实终态时间；活动操作为 (1,false)，成功终态在窗口内为 (1,false)，
失败终态在窗口内为 (0,false)，所有终态在窗口边界及之后为 (0,true)。
调用方提供与 operator 相同的宿主时钟，在状态刷新后采样当前时间；比较过程不修改原始负载。
它不承担 hot scheduler 的配置倍数、pending map 聚合、指标或垃圾回收循环。

`pause_and_resume_leader_transfer` 对 old−new 恢复，对 new−old 暂停，保留交集
的引用计数。使用真实 BasicCluster store registry，缺失 store 的 pause 错误按 ID
排序返回，继续处理其余 ID；Go 对相同错误记录日志，这里留给宿主记录。
恢复缺失 store 遵循 core 的 no-op 契约。不触及另一方向的计数。

来源是只读 `utils.go`、`hot_region.go::calcPendingInfluence` 和
`operator.go::CheckAndGetStatus`。`utils_test.go` 的 TestRetryQuota / TestGetCountThreshold
只覆盖前置范围与配额，不作为本任务的负载比较/终态运行证据。
`hot_region_test.go::TestGCPendingOpInfos` 提供活动/取消/到期的源码契约。
本次未运行 Go；冻结期望来自源码推导，详细来源哈希、Rust 行为和故障实测见
[task-069](../../../docs/rust/schedule-execution/task-069.md)。

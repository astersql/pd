# 调度器与检查器管理查询

`scheduler_admin.rs` 在已有 `Handler` 上提供 `get_schedulers_controller`、
`get_scheduler_names`、`get_scheduler_by_status`、`get_checker_status`、
`pause_or_resume_checker`、`get_diagnostic_result`；独立
`pause_or_resume_scheduler` 接受宿主的可变 Coordinator 借用，沿用现有管理输入
分派的借用方式，不复制控制器、配置、checker 或诊断 recorder。

`SchedulerQuery` 区分名称列表和暂停时间段。时间段使用精确 Unix 秒，JSON 日期
转换由后续传输适配层负责。只有 paused + need_timestamps 返回时间段；disabled
以及所有其它 status 返回名称，未知 status 按 Go 的 default 分支过滤 enabled。
暂停的 disabled scheduler 仍会出现在 paused 查询中。名称顺序沿用 canonical
controller 的升序；Go map 顺序未定义。没有隐藏错误、忽略状态或时间字段。

Checker 使用真实 dispatcher 的七类 pause controller，未知名称（包括 all）返回
`ErrCheckerNotFound`；scheduler 的 all 由原 controller 处理。时钟由宿主传入，
保持 int64 溢出及到期边界。暂停不会生成诊断记录；查询返回真实 recorder 最近状态，
其 timestamp 是查询时刻。仅 leader/region scheduler 可诊断，先验证名称、后检查
bootstrap，因此未知名称优先返回 `ErrSchedulerUndiagnosable`。其它管理查询先验证
bootstrap。`SchedulerAdminError` 保留原始 typed domain/control 错误。

Go 依据是只读 `handler.go`、`coordinator.go`、checker/controller 和 scheduler
controller/diagnostic recorder。按用户后续授权使用源码推导加 Rust 行为验证，未执行
Go oracle，未宣称运行差分。冻结输入与字面期望独立保存在 fixture/独立测试中；
[证据](../../../docs/rust/schedule-execution/task-122.md)记录失败、通过及完整验证。

测试装配真实 BasicCluster、Coordinator、工厂、MemoryStore、checker 与 recorder，
只控制时钟。管理查询/暂停自身不做 I/O，不需要伪造存储故障；测试比较真实持久化
内容及 revision 不变，并验证未知名称拒绝后期限未改变、无 bootstrap、禁用诊断和
缺 recorder/结果错误。无 HTTP/server/runtime 依赖或私有运行时。

# 调度器配置 HTTP 挂载

`router` 注册 `/pd/api/v1/scheduler-config` 及后缀；保留请求方法、原始 body、
查询串和局部处理器响应。`PdServices::scheduler_config` 把请求送入宿主既有
coordinator 队列，由 `schedule_scheduler::configure` 独占访问 canonical 策略。

18 类分发复用 `schedule_admin_*::serve`，不重新实现字段校验或持久化。
缺失实例使用 Go ServeMux 的普通404；未 bootstrap 返回406；外层只接受
Go 注册的 GET/POST/PUT/DELETE/PATCH，并保留公共 OPTIONS 预检200。取消沿请求令牌和宿主队列传播。

独立测试在 `lib.rs` 注册；真实网络、事务故障、重启及只读 Go 差分见
[任务65证据](../../docs/rust/server-api/task-065.md)。

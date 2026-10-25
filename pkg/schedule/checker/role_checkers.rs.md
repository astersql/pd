# 暂停与简单角色恢复

`role_checkers.rs` 是 canonical `astersql-pd-schedule-checker` 的首个行为单元。
`PauseController` 使用 atomic Unix 秒截止时间，TTL 更新覆盖旧截止时间，严格小于
截止时间才暂停，0/负 TTL 恢复，int64 加法按 Go wrapping 语义。宿主可使用
`pause_or_resume`/`is_paused` 读取系统时钟；显式 `_at` 方法供确定性时钟边界使用。
检查器采用同一 BuilderContext 的 `now_unix_nanos` 快照，按向下取整换成 Unix 秒。

`LearnerChecker::check` 遍历真实 RegionInfo 按 peer ID 排序的 learners，调用现有
PeerOperatorFactory；失败继续下一个，首次成功即返回。pending/down、无 leader、
placement 缺失和 joint origin 由 canonical Builder 验证。现有 peer 晋升无需分配 ID；
意外分配请求返回类型化 BuilderError，不能伪造新 ID。

`JointStateChecker::check` 对正常状态直接返回空；对 joint 状态调用现有
RangeOperatorFactory::create_leave_joint，包含需要的领导者转移，并设 HIGH 优先级。
其恢复路径按 Go 跳过 placement fitting。现有工厂保留无有效候选时仍发 leave 的源行为。

`RoleCheckOutcome.operator` 对应 Go Check 的可选操作。`errors` 保留 Go 中只写入
日志的 BuilderError（learner 的先前失败与后续成功可同时出现），供宿主自行诊断。
库不安装 logger、runtime 或全局状态；不维护第二份集群、placement 或操作规划。
本路径没有持久化/外部 I/O，不需要 MetaStore 故障替身。指标注册与 controller 接线
属于后续任务，不在本单元引入独立统计状态。

来源：checker_pause.go、learner_checker.go、joint_state_checker.go、对应 Go 测试、
operator/create_operator.go、operator/builder.go、core/region.go。用户已解除 Go 运行
准入，本任务 fixture 明确为 source_assertion，未运行 Go，不宣称运行差分。
冻结场景、来源哈希、失败和通过验证见
[task-039.md](../../../docs/rust/schedule-execution/task-039.md)。

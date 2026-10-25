# 检查器 crate 入口

`lib.rs` 是 `astersql-pd-schedule-checker` 的唯一库入口，禁止 unsafe，
重导出 `role_checkers` 中的 PauseController、LearnerChecker、JointStateChecker
和 RoleCheckOutcome。`task_039_test` 仅在 cfg(test) 中显式注册。

实现边界与来源见 [role_checkers.rs.md](role_checkers.rs.md)，验证记录见
[task-039.md](../../../docs/rust/schedule-execution/task-039.md)。入口不创建 runtime、
集群副本、持久化后端或协议适配；后续检查器应在本 canonical crate 中注册。

# 构建器领导者与替换择优

`builder_simple` 的局部子模块实现 canonical Planner 的择优方法，不建立第二个
Builder 或状态副本。`Builder::build_simple` 通过 `select_target_leader` 选择最终目标，
通过 `rank`/`best`/`replace` 规划替换步骤；不新增公开配置、依赖或 Cargo feature。

来源为只读 `builder.go` 的 setTargetLeaderIfNotExist、prefer*、comparePlan、labelMatch、
planReplaceLeaders 和 allowLeader。目标先排除非法 peer 和 ExpectedRoles.Follower，
按 leader role、preparing/serving、current leader、非待晋升 voter、较早添加的 peer
依次比较，再按 count/size leader score 从低到高比较。完全相同保留较早 store ID。
显式 leader 不参与重新择优，force 仍受 store 存在和 peer role/witness 限制。

替换计划按六维字典序比较：位置标签相同前缀、up snapshot 源、旧 peer、优先加入
目标 leader、目标 leader、减少 leader 迁移。标签复用 core::equal_fold，包含 Unicode。
新增 learner + promote 的位置分数取两个距离分数较小者；新加 peer 做 leader
额外罚 len(steps)+1。并列保留第一计划。非 joint 路径没有独立 demote 步骤，沿用
普通 Planner 的 remove/add 语义；joint 与 witness 转换属于后继任务。

内部 `allow_leader_at` 以 Planner.current leader 判断豁免，prepare 校验仍用 origin；
角色/witness 在豁免前检查。修正了前序实现将 origin 永久豁免的差异。
SetLeaders 排序、过滤后保留重复项，构建过程不重新过滤候选，所有 TransferLeader
携带同一候选列表。不执行存储 I/O，分配错误沿用 BuilderError，不新增 mock 后端。

[固定场景](../../../docs/rust/schedule-execution/fixtures/task_028.json) 与独立注册的
`task_028_test.rs` 检查真实 Operator 步骤、kind、候选列表、错误、输入所有权及
Planner 评分。依用户授权，Go 契约标为 source_assertion，未运行 Go oracle；
[证据](../../../docs/rust/schedule-execution/task-028.md) 记录来源哈希、行为回退失败、
通过测试、workspace 基线和共享 Cargo 槽位。

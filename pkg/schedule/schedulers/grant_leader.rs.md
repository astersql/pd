# grant-leader 领域调度

`GrantLeaderConfig` 持有 store→完整包含区域的 key ranges 快照；
`GrantLeaderScheduler::prepare/clean` 对 canonical BasicCluster 的 OUT 暂停引用操作，
Prepare 遍历所有 store，保留已成功的暂停并返回最后一个错误。协调者必须对失败的
Prepare 也调用 Clean；独立调用者的 IN/OUT 暂停不会被这次配对释放。

`is_schedule_allowed` 对 leader operator 数量使用严格小于限额；schedule 与 Go 一样
不重复限额检查。每个配置 store 至多选一个 follower，仅使用 pending/down 区域过滤器。
空 ranges 无候选，范围须包含整个区域，目标已经是 leader 不会被 follower 索引选中。
不同目标可能选择相同区域，不做 Go 没有的跨目标去重。

调度复用 canonical follower index、RuleManager、PeerOperatorFactory 强制转移路径。
强制转移跳过集群限额/placement 检查，但仍拒绝 witness、非投票角色和不存在的 store。
生成一步 OpLeader、High 优先级操作；保留 builder 类型化错误用于诊断（Go 日志后跳过）。
共享 context/result 复用前序 eviction 的通用边界容器，不复制集群状态或创建 runtime。
时间/随机输入由宿主提供；实际元数据和规则来自 canonical core/MetaStore 接口。

此任务仅提供调度、配置快照、Prepare/Clean 和限额 API；HTTP/配置持久化事务在后续
任务负责，不在该领域类型中引入网络路由或私有存储。

证据见 [task-077](../../../docs/rust/schedule-execution/task-077.md)。依据用户授权，
Go grant_leader.go、grant_leader_test.go 和 operator builder 只读推导用于冻结期望，
并以 Rust 真实对象和故障行为验收；没有 Go 运行 oracle 或运行差分声明。

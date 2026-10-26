# 慢网络候选判定

`evict_network_detect.rs` 是 canonical schedulers crate 的网络检测领域实现。
公开 `detect_network_slow_candidates` 返回排序后的候选 ID；
`build_problematic_network_map`、`should_skip_store_evaluation`、
`is_network_slow_store`、`filter_network_slow_scores` 分别对应 Go helper。

复用真实 `astersql_pd_core::BasicCluster/StoreInfo/StoreStats`。恢复参数只需
`HashSet<u64>`：Go 判定只使用恢复 map 的键，不读取 nil/时间值。
检测不修改恢复集合、暂停引用、磁盘配置、MetaStore 或 operator 状态；
后续网络驱逐/恢复任务负责这些有副作用的动作。没有新增依赖、feature 或协议类型。

问题图是对称的，报告者至少有两份过滤后评分才建立 >=95 的边。
候选至少有三份评分，需满足全部 live peer 共识，或非问题边中至少两个 >=10
的评分。零分仍计入评分数量。未知目标保留，确认 removed 的目标和恢复集合键
被过滤；报告者/候选/共识分母只使用 live population，Removing 仍属于 live。
恢复集合中的非 live ID 不从分母再次扣除。保留 Go 对有符号分母的比较语义。

集合顺序无语义，候选返回排序仅用于稳定消费；不裁剪未知目标、异常大评分、
自报告或其图边，避免悄悄改变 Go 行为。检测完成前重新查询候选是否注册且未移除，
与 Go 一样只能缩小并发移除窗口，并不宣称锁住后续执行。
不存在本任务自己的 I/O 错误或持久化故障注入点；缺失心跳评分、节点移除及
未知评分目标以真实 core 对象验证，不以 mock 调用次数替代结果。

依据为只读 Go `evict_slow_store.go` helpers 和相邻网络测试，按用户授权使用
源码推导期望；没有取得 Go 运行 oracle/差分。固定输入、阈值红灯、22 个判定案例、
图/跳过/分母/心跳丢失测试及 workspace 验证见
[任务80证据](../../../docs/rust/schedule-execution/task-080.md)。

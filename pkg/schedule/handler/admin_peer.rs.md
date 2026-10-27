# 管理领导者与副本操作

`Handler::add_peer_operator` 接收 `AdminPeerRequest` 的 TransferLeader、
TransferRegion、TransferPeer、AddPeer、AddLearner、RemovePeer 六类领域意图。
`pre_check_for_region` 返回真实 RegionInfo 或类型化错误；HTTP 状态转换由适配层负责。

Handler 借用 Coordinator 的 canonical cluster/config/controller，复用
PeerOperatorFactory 的普通/联合共识规划，不另建 peer planner。宿主提供适用的
RuleManager apply-range rules、集群版本能力、时钟及 ID allocator。TransferRegion
省略角色仅在 placement 关闭时补 voter；开启时缺角色提前返回。角色顺序显式保留，
与源工厂按 map 迭代位置继承 witness 的行为相容。TransferPeer 保留原 role/witness。

先验证 region/peer/store，再构建 Admin 操作；移除和失联 store 拒绝；allocator、
placement、leader eligibility 与 admission 错误透传。成功操作具有已有工厂提供的
Admin/URGENT 语义。控制器替换移除的操作经既有 ack/records 路径处理。无私有 Tokio
runtime、协议类型、HTTP/server 依赖或新的持久化写入。

只读来源为 `handler.go:408–578,720–745,1324–1341`、`create_operator.go`、
`builder.go` 与 `tests/server/api/operator_test.go`；期望为源码推导，未声称 Go
运行 oracle。用户明确允许以源码推导及 Rust 真实行为/故障验证替代缺失 Go 运行证据。

[任务119证据](../../../docs/rust/schedule-execution/task-119.md)保存 fixture provenance、
红灯、聚焦/包级/workspace 命令、退出码及测试数。独立注册的 task_119_test.rs 验证
具体步骤/目标/peer ID/kind/优先级/运行状态，以及所有错误不入队和保留已有操作。

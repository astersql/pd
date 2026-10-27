# 调度心跳 wire 适配

`schedule_wire` 是 `astersql-pd-server` 的公开适配模块，依赖协议无关的
`astersql-pd-schedule-hbstream::HeartbeatCommand`，复用既有 core region/peer 转换。
领域 crate 不反向依赖 server/proto；没有新增 feature、运行时、持久化或网络路径。

`to_pdpb` 和 `to_scheduling` 借用命令并返回拥有字段的协议响应，全部七种
Operation 字段独立转换，不解释为 oneof，不检查调度决策合法性。
保留 absent/present-empty、显式 false、空 joint batch、重复/有序 split keys、
peer role/change type/policy 原始枚举值、完整 merge region 和 epoch。
两种响应的头和动作类型相同，但 region、epoch、target、change、transfer 的
字段编号不同，必须各自构建并由对应生成类型编码。

`keepalive_pdpb`/`keepalive_scheduling` 只有 present header；
`error_pdpb` 对应 Go `SendErr`，即使 scheduling stream 也使用 PD 错误响应。
错误类型保留原始 i32，target 可缺失，region/epoch/actions 为默认。
不额外发明 scheduling SendErr 路径。无 leader 时由领域
`HeartbeatCommand::for_region` 抑制发送，适配器只接受已有 leader 的命令。

Go `TransferLeader`、`ChangePeerV2`、`BatchSwitchWitness` 生成 marshaler 会
解引用 repeated message 中的 nil。prost 的 repeated message 不表示 nil。
适配器拒绝这些不可编码输入，返回 `WireError { field, index }`，不丢弃元素或
悄悄转换成默认消息；错误不修改借入的命令，也不返回部分响应。
单个 optional peer/target 缺失保持缺失，present 默认消息保持 present。

按用户明确授权，证据为只读 Go 源码/协议推导及 Rust 真实转换与故障验证，
不是新增 Go 运行 oracle。来源是 `heartbeat_streams.go` 的 SendMsg/SendErr/keepalive，
固定 kvproto 的 pdpb/schedulingpb/metapb/encryptionpb 定义及生成 Go marshaler。
[任务证据](../../docs/rust/schedule-execution/task-134.md)记录源哈希和限制；
[独立冻结器](../../docs/rust/schedule-execution/logs/task_134/freeze.py)从协议字段号
生成固定期望字节，既不调用 Go 也不调用被测 Rust。测试解析固定输入，逐字节比较
两种响应并使用 prost 解码比较完整消息。覆盖全部128种 presence组合、正常动作、
边界枚举、错误头、nil拒绝、畸形输入解码和字段丢失变异。

# Peer/leader 操作工厂

`PeerOperatorFactory` 在 canonical Builder 上记录一次请求的目标，消费自身后返回
真实 `Operator`。公开 `create_add_peer`、`create_remove_peer`、
`create_promote_learner`、`create_demote_voter`、`create_transfer_leader`、
`create_force_transfer_leader`、`create_move_peer`、`create_move_region`、
`create_move_leader`、`create_replace_leader_peer`，以及 promote/remove、demote/remove
组合入口。没有第二套步骤规划、运行时、协议转换、全局状态或持久化路径。

宿主传入真实 `BuilderContext`、`RegionInfo`、ID allocator 和
`supports_conf_change_v2`。最后一项是宿主根据集群版本得出的能力值（Go 的
ConfChangeV2 门槛为 5.0.0），不是 Cargo feature；同时开启能力与配置才选择
`build_joint`，否则使用 `build_simple`。轻量 snapshot 选项保留默认 false。
单项变更回退等规则由已有 Builder 处理，工厂不复制。

普通转移跳过 origin joint 检查；强制转移还跳过 placement fitting 并启用
force target。两者都保留 peer 身份/角色/健康校验；强制也要求目标 store 存在。
联合状态中的 incoming voter 可接任，demoting voter、learner 和 witness 不可接任。
其他工厂保留完整默认准入。错误返回已有 `BuilderError`；demote/remove 组合
在配置关闭 joint 时先返回 `JointConsensusDisabled`，再进行任何目标校验或分配。
allocator 的实际错误原样上抛，失败不返回半成品操作。

MoveRegion 接收有序 `(store, PeerRoleType)` 列表。Go 遍历 roles map 时按索引
继承原 peers 的 witness 标志；有序列表显式表达该输入顺序，避免领域库安装
随机源或把 witness 错误地按 store 匹配。随后转为 Builder 的 canonical maps。
原 peer 身份由 Builder 保留，新零 ID peer 才调用宿主分配器。重复 store 为
非法输入。调用者传入的 region、peers 和 config 不被修改。

依据 `create_operator.go` 的对应工厂和 `builder.go` 的规划契约；本次是源码推导
验证，未执行 Go oracle、未声明 Go/Rust 运行差分。测试、固定输入/输出和失败证据
见 [任务31证据](../../../docs/rust/schedule-execution/task-031.md) 与
[task_031_test.rs](task_031_test.rs)。真实状态心跳回放检查完整步骤、kind、
目标/身份、最终 peers/leader、完成与 ConfVerChanged、命令类型以及影响量入口。

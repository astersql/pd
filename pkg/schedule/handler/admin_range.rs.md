# 合并与拆分管理操作

`Handler::add_merge_region_operator` 依次验证 source、target、健康/复制和非空边界
相邻性，再调用 canonical `RangeOperatorFactory`，通过真实 ControllerAdmission 原子
接纳 active/passive 两个操作。宿主借入 `AdminRangeContext` 的 RuleManager、时钟、
集群版本能力及 allocator；读取 Coordinator 的同一配置与集群。复制判定复用
`is_region_healthy`/`is_region_replicated`，placement 开启时使用真实规则拟合；
工厂使用 source apply-region rules。接纳时重新核对双方 epoch，替换操作走已有
ack/records 路径。没有独立元数据副本或持久化写入。

`Handler::add_split_region_operator` 接收领域 region ID、策略名和 hex 字符串。
策略支持大小写无关的 SCAN、APPROXIMATE、USEKEY；只在 USEKEY 下解码，拒绝奇数
长度、非 ASCII hex 字符。空键、空列表、重复和乱序键照原样交给工厂；SCAN 与
APPROXIMATE 忽略 keys。保留 Go 的校验边界：split 不额外拒绝 down/pending、
副本不足和无 leader，但真实工厂拒绝联合状态。策略和键错误先于工厂联合状态错误。

`AdminRangeError` 区分未初始化、region 缺失、异常 peer、非相邻、策略、hex、
Builder 与 Admission 错误，HTTP/JSON 映射留给适配层。成功操作保持 ADMIN、URGENT、
原始 epoch 和二进制范围，merge 的匹配前置步骤由已有工厂负责。

依据为只读 `handler.go:581–659`、`filter/healthy.go`、`operator/create_operator.go`
和 `tests/server/api/operator_test.go::checkMergeRegionOperator` 的源码推导。
用户明确授权以源码推导加 Rust 真实行为/故障验证替代缺失 Go 运行 oracle，未声称
获得 Go 运行差分。无 Go 文件、Go 模块文件或 plan.md 改写。

[任务120证据](../../../docs/rust/schedule-execution/task-120.md)保存 fixture、来源哈希、
行为红灯、聚焦/包级/workspace 命令与测试数。独立注册 `task_120_test.rs` 使用真实
BasicCluster、Coordinator、工厂、控制器和 MemoryStore/RuleManager；仅控制宿主时钟、
能力与 allocator I/O 边界。Handler 不执行元数据写入，未虚设持久化故障接口。

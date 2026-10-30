# 操作创建输入分派

`OperatorCreation::parse` 把 JSON 值转换为六类领域意图，包含全部十种 Go action。
`handle_operator_creation` 接收宿主可选的可变 Coordinator、CreationContext 和 ID
allocator，执行真实 peer/range/scatter 管理动作。`CreationResponse` 保存兼容的数字
状态码、JSON body 和独立类型化错误；HTTP 路由/错误序列化仍由宿主适配器负责。
没有 server/proto 反向依赖，也不建立私有 runtime、规则、计数或控制器。

解析先于 bootstrap 检查。name、必需 ID、store 数组、policy 和 keys 按来源顺序校验。
roles 只有数组长度匹配时生效；非字符串 role 保留先前值；重复 store 合并，后续字符串
覆盖先前字符串。缺角色保留 Unknown(空字符串)，交给前序管理 API 的 placement
校验。类型错误的可选 group/start/end 采用空字符串，retry_limit 类型错误采用5；
任务063的真实 HTTP 差分补充了数字契约：ID与retry_limit先经float64舍入，再按固定
Go amd64 oracle执行截断；负数ID保留有符号转无符号结果，越界转换保留该oracle的
2^63哨兵值，retry_limit越界采用i64::MIN后收敛到[-1,30]。这替换了任务121最初
对分数/负数/越界值返回400的输入边界；NaN/Infinity仍由JSON层拒绝。
keys非数组返回400，替代Go类型断言panic。未bootstrap的Go单区创建还存在typed-nil
panic；Rust保持结构化500，差分明确记录双方原始响应，不宣称错误体等价。

peer/merge/split 复用119/120的工厂、验证和 admission/归档。
单区与批量 scatter 委托同包 `scatter_admin.rs`，复用 canonical `ScatterBatch::scatter_one`
及 Coordinator 的 RegionScatterer；单区按 Go addOperator 直接准入，归档并确认替换项，
空操作成功。宿主转发 suspect IDs 到现有 checker，真实操作保留 ADMIN/HIGH。批量保留
计数及 pending 回调；两端非空 hex 范围优先于 IDs。错误总是200，body包含
`processed-percentage` 与 `error`；分区失败保留 Go 整数百分比计算，不上抛为500。

只读推导依据为 `handler.go:246–405,655–718,746–774`、
`typeutil/conversion.go:55–72` 及既有 API 测试。不存在本任务 Go 运行差分声明。
用户授权允许以源码推导和 Rust 真实行为/故障证据替代缺失运行 oracle。
[任务121证据](../../../docs/rust/schedule-execution/task-121.md)记录 fixture、provenance、
红绿灯、命令、退出码、测试数和实际槽位。

当前HTTP接线、数值运行差分与故障证据见[任务063](../../../docs/rust/server-api/task-063.md)。

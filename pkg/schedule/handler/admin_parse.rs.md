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
有限 retry 数字按 Go int64 截断，执行时等效收敛到 [-1,30]，与现有 scatter 上限一致。
ID 先经 float64（与 Go JSON decoder 相同），再检查有限、非负、整数且小于 2^64；
超出精确表示范围的整数保留 Go float64 舍入语义，不声称原始十进制无损。

任务明确要求非法浮点 ID 校验，故负数、分数、越界 ID 返回400，不模仿 Go 未验证的
float→uint64 转换。keys 非数组返回400，替代 Go 类型断言 panic；这两项是显式安全
输入边界。serde_json 不表示 NaN/Infinity，它们在 JSON 解码层已拒绝。

peer/merge/split 复用119/120的工厂、验证和 admission/归档。
单区 scatter 复用 canonical `ScatterBatch::scatter_one`，通过局部公开该已有规划入口
避免复制算法；宿主转发 suspect IDs 到现有 checker，真实操作保留 ADMIN/HIGH，按 store limit
接纳并归档替换项，空操作成功。批量散射复用 Coordinator 的 RegionScatterer，保留
计数及 pending 回调；两端非空 hex 范围优先于 IDs。错误总是200，body包含
`processed-percentage` 与 `error`；分区失败保留 Go 整数百分比计算，不上抛为500。

只读推导依据为 `handler.go:246–405,655–718,746–774`、
`typeutil/conversion.go:55–72` 及既有 API 测试。不存在本任务 Go 运行差分声明。
用户授权允许以源码推导和 Rust 真实行为/故障证据替代缺失运行 oracle。
[任务121证据](../../../docs/rust/schedule-execution/task-121.md)记录 fixture、provenance、
红绿灯、命令、退出码、测试数和实际槽位。

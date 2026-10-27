# 领导者授予配置 HTTP 边界

`serve` 独占借用宿主 `GrantLeaderScheduler`，将相对 `/config` POST、
`/list` GET、`/delete/{store_id}` DELETE 转为 canonical grant 配置事务。
宿主负责完整路由挂载、同步与删除 scheduler 回调；不创建 runtime 或重复领域状态。
`astersql-pd-core-constant` 从测试依赖提升为生产依赖，仅用于 Go 缺失 ID 错误路径
释放 store 0 的 OUT pause，与已有集群引用计数契约一致。

Go 来源为 `pkg/schedule/schedulers/grant_leader.go` 的 handler 和
`grant_leader_test.go:TestGrantLeaderScheduler`。用户授权以只读源码推导及 Rust
真实行为/故障测试替代缺失 Go 运行 oracle；未运行 Go，不主张运行差分。
BaseScheduler 的 `not implements` 被专属 ServeHTTP 覆盖，不用于此 scheduler。

POST 的 `store_id` 按 Go map 的 float64 接收，正小数截断；键区分大小写，重复键
最后值生效。`ranges` 的 `[]string` 断言对 JSON 解码数组不成立，因此忽略输入，
已有范围重新 QueryUnescape、新 store 默认全范围。
错误片段从原始字节定位，不能用领域层百分号编码后的偏移直接切片非 ASCII 范围。普通配置错误为 JSON 字符串；
JSON 类型/词法错误为 apiutil 结构体和 input header；读流失败为 internal 响应，
保留非空 stack，但不比较非确定栈帧。方法不匹配 405 空体，无 Allow；未知路径404。

使用真实 BasicCluster、MemoryStore/StorageEndpoint 与 canonical 更新/删除/reload。
更新保存失败删除对应内存条目并释放 OUT pause，旧磁盘值仍可恢复；删除保存失败
保留原状态，最终删除回调失败恢复内存但不撤销已写磁盘，NotFound 不恢复。
外部 I/O 注入 Unavailable 的错误字符串来自 MetaStore 契约，不能冒充 Go 后端错误。
GET 返回克隆配置，空 range 列表映射 null，不修改领域值。

验证见 [task-145](../../docs/rust/schedule-execution/task-145.md)，独立测试模块显式
注册。固定 fixture 比较状态、响应字段、Content-Type；另测持久化故障、读流错误、
删除回调、保留范围解码错误及 HTTP 更新/reload 后的真实 HIGH LEADER 操作。
不主张覆盖 Go 对超范围/负 float 到 uint64 的架构相关转换、每一种 encoding/json
词法错误或 Gorilla mux 的所有路径 canonicalization；固定案例以外仍需专项协议扩展。

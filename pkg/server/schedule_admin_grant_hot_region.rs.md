# schedule_admin_grant_hot_region.rs

`serve(&mut GrantHotScheduler, &StorageEndpoint, &ClusterPaths, Request<Body>)`
实现 scheduler 相对 POST `/config` 和 GET `/list`。宿主串行借用 canonical 调度器，
借用覆盖存储 I/O；不增加运行时、全局路由、feature 或领域实现副本。

Go grant_hot_region.go 覆盖 BaseScheduler.ServeHTTP，存在专属接口。配置嵌入的是
不导出字段的 schedulerConfig 接口，不是 disabled base 配置，list/persist 只有
store-id 与 store-leader-id。默认 list 克隆出空数组；成功 POST 响应 JSON null。

按 store-id 字符串类型、各十进制 u64、leader 字符串类型、leader u64 的顺序校验。
空项、符号、空白和越界拒绝；保留重复和顺序，缺失 leader 时自动补入 store 列表。
错误使用 Go 原始 Normalize 错误的 JSON 对象，保留未格式化消息模板与 RFC code。
合法输入交给已有 update_config。真实保存失败保留新列表并清零 leader；reload 可
恢复旧持久化值。读取错误先于解析处理，JSON 错误复用相邻兼容诊断函数。

契约采用用户授权的只读 Go 源码/测试推导，没有 Go 执行或运行差分证据。
JSON 只归一化键序、缩进和尾随换行；真实故障栈不跨语言比较。完整 server/api
装配、Gorilla 路径清理和重定向属于后续工作，本入口只处理相对路径。

独立注册测试包含表驱动 HTTP、MemoryStore I/O 故障、真实调度器/placement/operator
和协议命令。固定时钟及随机目标，不声称随机分布或 Go map 顺序等价。
详见[任务143证据](../../docs/rust/schedule-execution/task-143.md)。

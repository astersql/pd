# 见证均衡配置 HTTP 适配

`serve(&mut BalanceWitnessScheduler, &StorageEndpoint, &ClusterPaths, Request<Body>)`
提供相对路径POST `/config`、GET `/list`，宿主负责互斥和挂载；不添加全局路由。
Gorilla路由的方法不匹配返回405空body，未知路径返回404明文。
JSON响应使用application/json; charset=UTF-8，比较时只归一化对象顺序及缩进。

只读Go `balance_witness.go` 的update、clone和handler固定batch 1–10、
大小写字段匹配、未知字段、相同配置响应、无效batch回滚、JSON类型错误和
持久化失败仍发布内存配置的行为。没有使用BaseScheduler不支持响应。
请求体I/O故障按Go忽略读取错误，用已取得字节解码；结构语法错误不修改配置。
字段类型错误保留已成功赋值字段，KeyRange自定义解码错误立即停止，
重复字段按输入顺序处理。list的clone将null ranges转为[]，持久化保留原始null。

canonical BalanceWitnessConfig增加精确传输snapshot（可空ranges/带符号batch）
以保留Go错误路径的中间状态，没有创建第二份领域配置。原new入口继续验证，
空输入规范化为全域。传输snapshot不验证；领域update额外验证batch，保存失败
仍回滚；HTTP独立按Go先发布再保存，错误不改变HTTP成功响应。
reload仍完整验证batch后发布，保留已保存null/[]表示。
既有纯JSON/key-range帮助函数提升为crate可见，未改变leader适配行为。

调度使用真实BasicCluster、RuleManager、OpInfluence及角色切换操作器，
时钟/随机在边界固定。MemoryStore测试后端只在MetaStore I/O入口注入不可用，
验证内存、持久化旧值、重载和恢复保存。没有Go代码/模块改动、运行时或feature新增。

证据为用户授权的只读Go源码/测试推导与Rust真实行为/故障验证，
不声称Go运行差分；来源/fixtures/红绿/包级及workspace验证详见
[任务139证据](../../docs/rust/schedule-execution/task-139.md)。

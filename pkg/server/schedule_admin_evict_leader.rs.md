# 领导者驱逐配置传输适配

`serve` 接受真实 `EvictLeaderScheduler`、`BasicCluster`、`StorageEndpoint`、
`ClusterPaths`、HTTP request及宿主移除回调，处理相对路径 POST `/config`、
GET `/list`、DELETE `/delete/{store_id}`。宿主管理独占访问、挂载和调度器移除；
本文件不装配全局 server/api 路由、不创建 runtime 或复制领域状态。

验证顺序为 IDs、batch、ranges；Go 的动态类型错误映射保留字符串和状态码。
空对象、null和未知字段仍保存当前配置。空ranges保留已有范围；新增store默认全域。
更新和删除均复用任务076事务，失败保留配置和暂停引用；最后store删除回调失败
恢复内存及暂停，已保存的空配置保持不变。NotFound回调不恢复。
GET通过克隆序列化把空范围切片表示为null，不改变领域或持久化快照。

JSON解析错误带 `TiDB-Error-Code: input`，使用Go errcode结构和可审核错误data。
读取请求体失败带internal code并返回500，不处理已经读到的JSON；栈为真实Rust
backtrace位置，语言相关调用位置按证据规则归一化。持久化后端错误保留实际
canonical错误字符串，不伪造Go后端错误。未知路径404明文；方法错误405空body。

[任务140证据](../../docs/rust/schedule-execution/task-140.md)和固定fixture记录只读Go
源码推导，未声称Go运行差分。适用默认features；沿用任务076的有效持久化schema
和float64 ID边界（拒绝2^64，Go超范围转换不保证平台无关行为）。不声称任意
JSON语法诊断、Gorilla路径清理/编码跳转或损坏存储快照均已穷举。

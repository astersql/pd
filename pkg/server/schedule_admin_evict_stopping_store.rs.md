# 停止节点驱逐配置 HTTP

`serve` 适配宿主挂载的相对路径 POST `/config`、GET `/list`，借用 canonical
`EvictStoppingScheduler`、StorageEndpoint、ClusterPaths 和宿主 disabled 值。
独占借用覆盖保存过程；不安装路由、运行时、全局状态或新 feature。

依据只读 `evict_stopping_store.go` 的 handler/updateConfig/listConfig/persistLocked
及 apiutil JSON 错误契约推导；用户授权以源码推导加 Rust 真实验证替代 Go 运行
oracle。未运行 Go，不宣称运行差分。此 scheduler 有专属 ServeHTTP，故
BaseScheduler 的 `not implements` 不适用。

batch 类型须为数值，范围 [1,10]，小数截断；缺失/null/未知字段仍保存，重复键
最后值生效。更新通过领域 `update_http` 保存后发布快照，保存失败不改变状态。
该方法允许省略 batch 时保留加载的历史值（如100），并保存宿主 disabled envelope。
请求 disabled 不覆盖宿主；Go list 新建零值嵌入配置，故返回 disabled=false。
list 保留真实 evicted-stores，更新不清理驱逐引用。

JSON 输入错误400/input，读取失败500/internal；保存失败500字符串，未知路径404
明文，方法不符405空body且无Allow。JSON仅归一化键序、缩进、尾随换行；真实栈帧
不跨语言比较。完整 server/api 挂载与 Gorilla 路径清理/重定向不在本任务范围。
测试覆盖固定输入、边界、读取/存储故障及更新后的真实操作器。证据见
[task-142](../../docs/rust/schedule-execution/task-142.md)。

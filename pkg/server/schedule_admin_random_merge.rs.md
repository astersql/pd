# 随机合并配置传输边界

`schedule_admin_random_merge::serve` 接受 canonical `RandomMergeScheduler` 共享引用和
axum 请求，沿用 Go `randomMergeScheduler` 嵌入的 `BaseScheduler.ServeHTTP`：
任意方法/相对路径返回200、`text/plain; charset=utf-8`、精确字节 `not implements`。
没有 JSON 响应、请求解码、配置修改或持久化入口，因此非法JSON与存储故障不会引出400/500。
不读取请求流；领域 scheduler 不依赖 HTTP。完整路由与 HEAD 线上body抑制由宿主负责。

依据用户授权，本任务使用只读 Go 源码/测试推导，未执行 Go 或声称运行差分。
[验收证据](../../docs/rust/schedule-execution/task-147.md) 固定来源与fixture哈希。
独立测试覆盖57个HTTP组合、4个不可读取请求体，以及真实状态/规则/统计/MemoryStore
上的12次调度决策；POST无效后保留范围和持久化字节，合并目标、优先级、步骤与heartbeat一致。

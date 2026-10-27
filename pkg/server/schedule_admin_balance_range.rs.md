# 范围均衡作业 HTTP 适配

`schedule_admin_balance_range::serve(&mut BalanceRangeJobs, JobTime, Request<Body>)`
提供 scheduler 相对路径分派，复用 canonical `BalanceRangeJob::from_http`、
`BalanceRangeJobs::add/delete` 和宿主提供的时钟/MetaStore。完整 server 路由、认证、
锁、HEAD 的线上 body 抑制和 runtime 归宿主，不在领域层引入 HTTP 或 proto 依赖。

依据只读 `pkg/schedule/schedulers/balance_range.go`，此 scheduler 覆盖 ServeHTTP，
有专属 handler，不能返回 BaseScheduler 的 `not implements`。

| 方法/相对路径 | 行为 |
| --- | --- |
| POST `/config` | 400 JSON 字符串 `update config is not supported`，不读 body |
| GET `/list` | 200 JSON 作业数组，空队列为 `[]`，不读 body |
| PUT `/job` | 验证参数，新增 pending 作业并持久化，200 `null` |
| DELETE `/job?job-id=N` | 取消作业并持久化，200 `null`，不读 body |
| 已知路径的其它方法 | 405 空 body，无 Allow |
| 未知路径 | 404 `text/plain; charset=utf-8`，`404 page not found\n`，`X-Content-Type-Options: nosniff` |

Go 验证顺序是 engine、rule、alias、timeout、范围，再检查 alias 重复和保存。
HTTP 层保留 Go 错误文案，域层负责实际对象及持久化；直接渲染领域 Argument 的文本
会丢失 Go `%!v(MISSING)` 等诊断，因此在适配层转换。timeout 使用 canonical typeutil
Duration；范围使用 canonical keyutil 与 domain 解码，保留双重 URL unescape、逗号
拆分、任意字节、plus 到 space 等语义。job-id 按 Query().Get 取第一个合法解码项，
ParseUint base 10 拒绝符号、空格、下划线和溢出。

新增失败返回 400 JSON 错误字符串；取消不存在/已完成或 I/O 失败的作业返回 500 `{}`：Go 的 FastGenByArgs
通过 SuspendStack 返回 withStack，包装器没有 JSON 导出字段/MarshalJSON，因此不会
调用内部 Error 的 JSON 编码。不能将它改成 class/code/message/rfccode 或错误字符串。请求读失败为 500/internal，JSON 读/类型失败为
400/input，均保留 tidb-error-code。配置/列表/错误方法/无效取消请求不读外部 body。所有 JSON 响应保留
unrolled/render v1.0.1 在 IndentJSON 模式追加的末尾换行，404/405 单独处理。

独占可变借用跨越检查、I/O 和发布。保存失败或提交前取消 future 保留本地快照；
测试使用 MemoryStore，仅在 MetaStore I/O 边界注入 unavailable/pending。后端提交后
响应丢失的通用不确定性继续服从 MetaStore 契约，不声称此适配提供分布式回滚。

按用户授权，以 Go 只读源码推导加 Rust 真实行为/故障验证替代缺失 Go 运行 oracle；
没有 Go 测试执行或运行差分声明。固定 fixture 比较 status/header/原始响应字节，
结构化错误和列表比较全部字段，仅忽略 JSON key 顺序/空白。时钟固定 UTC 纳秒，
没有忽略时间/操作/持久化字段。任意损坏 JSON 的逐字节错误诊断及 Gorilla 路径清理
重定向不包含在固定案例一致性声明中。详见[任务证据](../../docs/rust/schedule-execution/task-154.md)。

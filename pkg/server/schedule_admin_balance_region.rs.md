# 区域均衡配置 HTTP 适配

生产入口 [serve](schedule_admin_balance_region.rs) 接受真实 canonical
`BalanceRegionScheduler` 的只读借用与 HTTP Request，返回 Response。
Go `balance_region.go` 没有覆盖 `ServeHTTP`，继承 `BaseScheduler` 的
200、`not implements` 明文响应；内容类型为 `text/plain; charset=utf-8`。
该固定响应是完整的不支持接口契约，不表示配置 API 已实现。

任意方法、局部路径、query 和 JSON（包括非法 JSON）均不参与分派或解码；
不读取请求体，不更新领域配置，不调用 MetaStore。
HEAD 在 handler 层仍写正文，宿主 HTTP 传输层负责线上正文抑制。
完整 server/api 路由属于主题7，本模块只暴露局部适配入口。

[测试](schedule_admin_balance_region_test.rs) 验证15项 HTTP 请求，1项外部请求体
I/O不可读取案例，以及真实调度在尝试 HTTP 更新前后仍生成 region1 从 store1
到 store4 的 AddLearner/RemovePeer 操作、pending influence 不变、配置存储为空。
没有专属更新入口，持久化失败/动态更新/reload 不适用；不增加不存在的能力。

Go 证据为用户授权的只读源码推导，没有 Go 运行差分。
[验收证据](../../docs/rust/schedule-execution/task-138.md)记录来源、红绿测试、
前置137和72回归，以及全workspace检查。

# 标签调度器配置传输边界

[schedule_admin_label.rs](schedule_admin_label.rs) 的 `serve(&LabelScheduler,
Request<Body>) -> Response` 接入 canonical LabelScheduler，复用宿主的调度器实例。
宿主负责挂载相对路径、同步和 HEAD 的线上响应体抑制；本文件不装配主题7的完整路由。
领域层不依赖 axum、protobuf 或 server，也不复制配置或创建运行时。

Go `labelScheduler` 嵌入 `BaseScheduler` 且没有覆盖 `ServeHTTP`。
继承方法忽略请求，直接写入 `not implements`；因此任意方法、路径、请求内容均返回
200、`text/plain; charset=utf-8`、原始文本，无尾换行、JSON 或 Allow header。
这不是缺失实现的占位响应，而是既有端点契约。没有动态 range 更新或持久化动作；
请求解析、body 读取错误、后端保存失败分支不可达，不额外发明错误响应。

[任务证据](../../docs/rust/schedule-execution/task-146.md) 和固定 fixture 标明
Go 源码/测试推导，无 Go 运行差分。真实 Rust 测试涵盖方法/路径/非法输入、nil/空/非空
ranges、不可读取的请求流、请求前后的真实调度及不变的真实 MemoryStore 配置。
测试单独置于 [schedule_admin_label_test.rs](schedule_admin_label_test.rs)，由 lib.rs 注册。

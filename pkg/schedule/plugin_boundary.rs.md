# Go scheduler plugin 边界

`Coordinator::load_plugin` 与 `Coordinator::unload_plugin` 同步返回
`UnsupportedGoPlugin`，通过 `PluginAction` 保留 Load/Unload 请求类型。
错误文本固定为 `Go scheduler plugins are not supported`。

这是 [ADR-001](../../docs/rust/architecture.md) 的明确不支持边界。
Go [PluginInterface](plugin_interface.go) 在 GetFunction 中解析绝对路径、打开
`.so` 并查找函数；[Coordinator](coordinator.go) 的 LoadPlugin 创建并添加
scheduler、启动等待 unload 的 goroutine，Unload 信号删除对应 scheduler。
Rust 不迁移 Go ABI，也不在这里引入其它扩展协议。

两个方法只接受 `&self` 和借用的 `&Path`，直接返回类型化错误；不转换、解析、
打开路径，不取得任何组件锁，不注册工厂、不访问存储、不启动后台任务。
空路径、NUL 路径、已有文件和内置 scheduler 同名路径都先到达相同边界。
不保存路径，也不把宿主传入的路径加入错误文本。

[任务证据](../../docs/rust/schedule-execution/task-117.md) 如实区分源码推导与
Rust 运行证据：按用户授权未要求 Go 运行差分，不声称 Go 自身拒绝 plugins。
[独立测试](task_117_test.rs) 使用真实 Coordinator、内置调度器、MemoryStore，
核对存储内容与 revision、暂停配置、checker 状态及 canonical owner 身份。

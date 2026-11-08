# 运维 45 的当前 Go 客户端回归

复用服务端 99 的真实 Go 客户端验收。同一轮分别启动固定协议 Go PD 和
当前 Rust `pd-server`。三个现有 Go 工具负责 bootstrap、三副本 region
心跳、Region 查询和 TSO 流。停止实际 leader 后，由原进程重新发现新
leader 并继续查询和分配。两侧 region、store、epoch 和成员观察均通过。
[既有用例](../../../tests/rust-compat/go_client_acceptance_test.rs)、
[场景与归一化规则](../server-api/task-099.md)。

| 服务器 | leader 序号 | Region 非零区间数 | TSO 非零区间数 |
| --- | --- | --- | --- |
| Go | 1 → 0 | 6 | 4 |
| Rust | 0 → 2 | 6 | 4 |

两个场景的三个工具均在取消后退出 0。区间数来自工具自己的成功请求统计。
`PD_COMPAT_MODE=dual cargo test --locked -p astersql-pd-compat-tests --lib task_099_ -- --nocapture`
退出 0，1 通过、0 失败、0 忽略、145 过滤。运行日志 SHA-256 为
`329978066f10fffb513f0654e78a56f06133f29982509bfe08420f0bb8640b4e`。

`go version -m` 确认三个工具是 darwin/arm64 的原入口，均使用固定
kvproto `f8ad079c8e98`，构建提交为 `35479179d0638445b91a3e4ff238057ba7896c87`，
与当前受保护的 Go 源码和模块文件无差异。Go PD 使用既有固定 amd64
oracle。Rust PD 使用同槽位构建的发行二进制。
槽位 1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`，
全部子进程退出后已释放。三工具及 Go 工作区沿用只读守卫。

此回执覆盖现有 Go client 的核心消费行为。运维 28–44 的 Rust client
交接和主题 9 总验收仍由其原有任务验收。

# Store 响应字段

`store.rs` 提供 `MetaStore`、`StoreStatus`、`StoreInfo`、`StoresInfo` 和
`build_store_info`。构造函数接受窄 `StoreResponseConfig`，复用 core 评分，
不依赖 schedule 或 protobuf。元数据采用独立快照；状态覆盖全部 Go JSON 字段。
ByteSize、Duration 复用 typeutil；时间输出 UTC RFC3339Nano，时区按计划归一化。
optional 字段遵循 Go omitempty；nil 列表、空列表和 nil 元素分别保留。
非法浮点及 RFC3339 年份范围错误返回序列化错误；非法 policy 保留 Go panic。
本批只提供原始 state_name；Down/Disconnected 判定由任务 67 扩展。

Go 依据为 `pkg/response/store.go`，相对基准
0820f0b67b0e49b289b84fdf66b7b8fb178afe82 无变化；元数据 JSON 标签来自固定
kvproto f8ad079c8e98 的 metapb.Store。fixture 记录逐场景计算和来源。
核心模型始终持有 raw report，builder 对应 Go newStoreStats 的非 nil 报告；
响应结构本身允许手工表达省略时间和 nil 指针。Go 人为设置 nil raw report
没有对应 core 输入，不宣称覆盖这一构造路径。

2026-10-23 只读运行
`go test ./pkg/response -run '^(TestEasyJSONCompatibility.*)$' -count=1 -v`，
退出 0，`TestEasyJSONCompatibility` 1 passed。该用例只验证 Region 往返，
Store 场景均为只读源码推导和 Rust 验证，不宣称跨语言运行差分。
未运行 make gotest 或任何 failpoint 改写命令，Go 文件与模块配置无改动。

TDD 首次 `cargo test --locked -p astersql-pd-response --lib core_task_66 -- --nocapture`
退出 101，原因是缺少 store 模块和真实公开类型。实现后同命令退出 0，
5 passed、0 ignored；`cargo test --locked -p astersql-pd-response --lib`
退出 0，16 passed、0 ignored。增加两个既有 workspace 依赖，Cargo.lock
仅登记 response 对 typeutil、core-constant 的依赖边，无版本变化。

完整验证：`cargo fmt --all`、`cargo metadata --format-version 1 --no-deps`、
`cargo fmt --all -- --check`、`cargo check --workspace --locked`、
`cargo test --workspace --locked` 均退出 0。workspace 76 个 target 汇总
436 passed，1 个既有 ignored；仅有既有 server cpu_usage 弃用警告。
`git diff --check` 退出 0。配置默认值核对 Go ScheduleConfig：count/v2/0.7/0.8。
全部 Cargo 命令使用动态槽位 1，绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
最终阶段使用退出/中断清理 trap，全部子进程结束后锁已释放，缓存保留。

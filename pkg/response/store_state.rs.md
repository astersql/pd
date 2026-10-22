# Store 响应状态名

`store_state.rs` 的 `build_store_info_at` 复用 task 66 的响应构建，接收调用方
Unix 纳秒时间与有符号纳秒 MaxStoreDownTime。保留 `build_store_info` 的前序字段
构建 API；需要实时状态名的调用方应使用新增入口。

依据 `pkg/response/store.go:BuildStoreInfo`（相对规划基准无差异）：只有旧
StoreState 为 Up 才覆盖状态名；严格超过最大下线时间先返回 Down，否则严格
超过 20 秒返回 Disconnected。Offline、Tombstone 和未知枚举数字字符串不覆盖。
`pkg/core/store.go:DownTime/IsDisconnected` 提供时间差和严格比较依据，Rust
复用已有 core 时间 API，含未来心跳负时长语义，不读取系统时钟。

`rust-fixtures/core_task_67.json` 的 11 个场景逐项记录源码依据，覆盖两个阈值
的等于/超过、Down 优先级、未来心跳、负阈值和非 Up 状态。另有调用方时钟前进
及回退测试。这些状态场景为源码推导，并非运行 Go/Rust 差分。
只读 `go test ./pkg/response -run '^(TestEasyJSONCompatibility.*)$' -count=1 -v`
退出 0，实际 1 个 TestEasyJSONCompatibility；该已有测试覆盖 peer/region JSON，
没有 Store 状态断言。未运行会改写 Go 的 make/failpoint 命令。

验证：共享槽位 1，CARGO_TARGET_DIR 为仓库绝对路径下的 target/rust-slot-1。
`cargo test --locked -p astersql-pd-response --lib core_task_67 -- --nocapture`
红灯 2 失败（Up 未覆盖为 Disconnected，cargo 退出 101），实现后 2 通过、0 ignore，
退出 0；`cargo test --locked -p astersql-pd-response --lib` 18 通过，退出 0。

完整验证（同一槽位）：`cargo metadata --format-version 1 --no-deps`、
`cargo fmt --all -- --check`、`cargo check --workspace --locked`、
`cargo test --workspace --locked` 均退出 0。workspace 共 438 通过、0 失败、
1 个既有 ignored（不属于 task 67），76 个测试目标含空 doc-test。
修改后先执行 `cargo fmt --all`；`git diff --check` 退出 0。

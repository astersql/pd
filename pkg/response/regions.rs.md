# Region 列表与取消

`regions.rs` 的 `marshal_regions_info_json` 接受领域 Region 引用列表与显式
取消回调，不创建运行时；返回完整 JSON 或 `RegionListError`，不暴露部分字节。
`RegionListCancellation` 区分取消与 deadline exceeded；错误文本对应 Go context。

Go 来源为 `region.go:MarshalRegionsInfoJSON` 和 `covertAPIRegionInfo`，相对
0820f0b67b0e49b289b84fdf66b7b8fb178afe82 无变化。count 是输入长度，保留顺序、
重复与 nil 条目；复用响应对象使 nil 保留前项，零 bucket version 保留前版本。
空列表不检查取消，非空列表每项转换前检查。固定数字和字符串字段没有可达的编码
错误输入；Rust 仍保留 serde_json 编码错误来源。fixture 逐场景说明来源。

只读运行 `go test ./pkg/response -run '^(TestEasyJSONCompatibility.*)$' -count=1 -v`
退出 0，1 passed；已有 Go 用例验证 JSON 往返，不覆盖列表和取消。
这些边界由 Go 源码推导并用 Rust 验证，不声称跨语言运行差分。未运行任何改写
Go 的 failpoint 命令，未改动 Go 源码、测试或模块配置。

TDD 红灯：`cargo test --locked -p astersql-pd-response --lib core_task_65 -- --nocapture`
退出 101，缺少 regions 模块、列表函数和取消类型。实现后同命令退出 0，3 passed，
0 ignored；`cargo test --locked -p astersql-pd-response --lib` 退出 0，11 passed。

完整验证（2026-10-23）：`cargo fmt --all`、
`cargo metadata --format-version 1 --no-deps`、`cargo fmt --all -- --check`、
`cargo check --workspace --locked`、`cargo test --workspace --locked` 均退出 0。
workspace 76 个 target 汇总 431 passed、1 个既有 ignored。check 仅有既有
server cpu_usage 弃用警告。`git diff --check` 退出 0。
动态领取槽位 1，全部 Cargo 命令继承
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；
全部子进程结束后释放锁，保留缓存。未新增依赖或修改 Cargo.lock。

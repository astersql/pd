# 单 Region 响应

`region.rs` 提供 `RegionInfo`、`APIRegionEpoch`、`ReplicationStatus`、
`new_api_region_info`、`init_region` 和 `marshal_region_info_json`。
领域值不依赖 protobuf。serde_json 是实际 JSON 输出所需的生产依赖；fixture 仅由测试读取。

Go 依据为相邻 `region.go` 的 `NewAPIRegionInfo`、`InitRegion`、
`fromPBReplicationStatus`、`covertAPIRegionInfo` 和 easyjson 字段省略规则。
相对于计划基准 0820f0b67b0e49b289b84fdf66b7b8fb178afe82，region.go 无变化。
现有 `TestEasyJSONCompatibility` 只验证现有对象的序列化往返；2026-10-23
直接运行 `go test ./pkg/response -run '^(TestEasyJSONCompatibility.*)$' -count=1 -v`
通过 1 个用例，退出码 0。未启用或改写 failpoints，未新增 Go oracle。

fixture `rust-fixtures/core_task_64.json` 记录各场景依据。
空输入、十六进制边界、最大 ID、bucket 复用及未知枚举是源码推导后的 Rust 验证，
不声称已运行跨语言差分。零 epoch 序列化为空对象；signed key 计数保持 Go u64→i64
转换；默认 leader 仍输出 role_name。复用时空 bucket 清除 keys，保留旧 version。

core 窄扩展保留 replication presence 和旧 CPU counter，clone_with 保留这两个值；
heartbeat 适配器读取 cpu_usage，区别于 unified_read。取消语义留给后续任务 65。

验证记录（2026-10-23）：领取槽位 1，所有 Cargo 命令使用
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，结束后释放锁并保留缓存。

- 红灯：`cargo test --locked -p astersql-pd-response --lib core_task_64 -- --nocapture`，
  Cargo 退出 101，缺少 region 模块/API 和所需 core getter/setter。
- 同一命令绿灯：退出 0，4 passed，0 ignored。
- `cargo test --locked -p astersql-pd-response --lib`：退出 0，8 passed。
- `cargo metadata --format-version 1 --no-deps`：退出 0。
- `cargo fmt --all`、`cargo fmt --all -- --check`：退出 0。
- `cargo check --workspace --locked`：退出 0。
- `cargo test --workspace --locked`：退出 0，76 个 target 汇总 428 passed，1 个既有 ignored。
- `git diff --check`：退出 0；Go 文件和 Go 模块配置无差异。

任务 68 补齐 `marshal_region_info_json_with_cancellation`：对应只读 Go
`MarshalRegionInfoJSON` 在转换前检查 ctx.Done，None 输入也先检查取消。
独立 `core_task_68_single_region_cancellation` 先缺入口失败（101），再通过；
覆盖取消、超时及无取消时复用原 JSON 结果。该证据为源码推导和 Rust 运行。

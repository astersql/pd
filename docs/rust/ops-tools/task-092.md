# 运维 92 的本地进程闭环

从运行中的 PD HTTP 和源 etcd 启动真实 `pd-backup`，读取归档并传给真实
`pd-recover`，在另一独立 etcd 上恢复。重复恢复退出 1。停止和重开目标
etcd 后，逐字节核对归档中的 ID 和 TSO 窗口。然后启动目标 PD，真实
`GetMembers` 返回 cluster ID 99，`AllocID` 返回 1629，高于归档的 1500，
`Tso` 返回 1794142520349 ms，高于归档窗口 1794142520348 ms。
这些数值来自最终进程测试。默认包级测试保留直接调用备份库的相邻覆盖。
[闭环测试](../../../tools/pd-recover/roundtrip_test.rs)。

| 命令 | 结果 |
| --- | --- |
| `cargo build --locked -p astersql-pd-backup-cli --bin pd-backup` | 0 |
| `PD_TEST_BACKUP_BIN="$CARGO_TARGET_DIR/debug/pd-backup" cargo test --locked -p astersql-pd-recover --test recover_roundtrip ops_092_roundtrip_process -- --ignored --exact --nocapture` | 待实现入口首轮退出 101，1 失败，实际备份进程退出 1。实现后退出 0，1 通过、0 失败、0 忽略、4 过滤 |
| `cargo test --locked -p astersql-pd-recover -- --nocapture` | 0，4 通过、0 失败、1 默认忽略。默认忽略项即上述已显式执行的进程测试 |

环境为固定官方 etcd 3.5.15 和固定 kvproto。槽位 1，绝对 `CARGO_TARGET_DIR` 为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`，结束后释放。Go 构建与只读
守卫详见运维 91 回执。 [只读回执](task-091.md)。

`cargo metadata --format-version 1 --no-deps`、`cargo fmt --all -- --check`、
`cargo check --workspace --locked` 和 `git diff --check` 均退出 0。整仓测试
由服务端验收任务 24 汇总，本轮运行受影响的两个包及显式进程闭环。

验收仍缺同源 Go 工具对照。收到可执行工件后须用隔离源和目标运行两个 CLI，
核对成功和拒绝的 stdout、stderr、退出码、备份字段和重启后键值，比较时只归一化
动态端口、临时路径和时间。Go recover 源码没有 Rust 的 `-alloc-timestamp`
参数，重复恢复分支返回 0。对照应分别记录共同的 cluster ID 与 ID 边界，以及
Rust 扩展的 TSO 和拒绝语义。 [Go 恢复入口](../../../tools/pd-recover/main.go)。

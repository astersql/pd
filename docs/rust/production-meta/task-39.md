# 生产任务 39 的本地同 etcd 切换回执

用固定协议 Go PD 启动 isolated 单节点，发送真实 GetMembers、AllocID、
Tso。用 SIGTERM 停止 Go 服务，从其原数据目录以官方 etcd 3.5.15
打开同一 WAL。Rust `pd-server` 通过显式外部 etcd 入口读取原元数据。
正常停止 Rust 后停止 etcd，再让原 Go PD 从同一目录恢复服务。Go 和 Rust
服务分时运行。etcd cluster ID `8597328664963232492` 在交接及回滚均一致，
PD cluster ID `7705791675057942424` 始终一致。

| 阶段 | PD leader member ID | AllocID | TSO（物理 ms，逻辑） |
| --- | --- | --- | --- |
| Go 初始 | 7347086966040775035 | 1 | (1794144436110, 1) |
| Rust | 7471611247234002913 | 1001 | (1794144441049, 1) |
| Go 回滚 | 7347086966040775035 | 2001 | (1794144444050, 1) |

Rust 和回滚 Go 的 HTTP health 均成功。两个 Go 服务及 Rust 服务均正常
退出 0，独立 etcd 由 SIGTERM 终止。原始记录含三阶段 gRPC 响应和退出值。
[本地原始记录](task-39.json.gz)。

`python3 /private/tmp/pd18-artifacts.hSESm1/cutover.py` 退出 0，1 个
实际 Go → Rust → Go 场景。槽位 1，
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
所有子进程已退出并释放锁。本轮是隔离单节点、同 etcd 后端的服务切换。
生产 39 的其余验收覆盖、生产 38 的去 etcd 转换、认证成员管理以及生产
发布仍按各自原任务验收。

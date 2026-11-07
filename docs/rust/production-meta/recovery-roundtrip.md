# Rust 恢复工具与备份往返

从 `2f7e9fe05` 推进运维86–90与92的恢复路径。`pd-recover` 提供独立 Rust
CLI，先校验参数，再用 etcd 原子事务恢复。新集群拒绝覆盖其他身份和下调 ID
或 TSO 边界。旧成员以 cluster ID、bootstrap key 及 alloc revision 为栅栏，
检查加法溢出，并与分配窗口更新一起删除该集群的 store 前缀。
[恢复入口](../../../tools/pd-recover/bin_main.rs)、[使用说明](../../../tools/pd-recover/README.md)。

备份库按现有 `clusterID`、`allocIDMax`、`allocTimestampMax`、`config`
格式导出。ID 与 TSO 从同一线性事务读取。真实往返用例从运行中的 Rust PD
HTTP 取得配置并落盘，解析备份后以实际 `pd-recover` 进程恢复至另一独立 etcd。
恢复后停启 etcd，并启动 Rust PD，验证相同 cluster ID 以及高于备份边界的
实际 ID 分配和 TSO 流。重复恢复返回失败，保留原状态。
[备份采集](../../../tools/pd-backup/pdbackup/collect.rs)、[集成用例](../../../tools/pd-recover/roundtrip_test.rs)。

`cargo test --offline -p astersql-pd-recover -- --nocapture` 退出码0。三个
有效用例覆盖 CLI 预检、副作用隔离、旧成员 ID 保护与备份恢复，0失败、0忽略。
实际槽位1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
真实 fixture 使用固定外部 etcd 3.5.15。本轮没有同源 Go `pd-recover` 运行回执。
Rust CLI 对无效输入返回非零，并扩展可选 `-alloc-timestamp`，因此不能声称
逐字节 CLI 行为等价。备份库由集成用例直接调用，独立备份命令与原生后端恢复演练待接线。

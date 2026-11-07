# 生产任务 30 本地原生恢复

`NativeMetaStore::backup_to` 在多数派读屏障后捕获一个已应用 revision，写入同目录
临时文件并同步后发布。归档携带校验、源共识集群 ID、业务记录和租约分配水位。
现有 Rust `pd-recover` 以 `--native-restore ARCHIVE NEW_DIRECTORY NEW_CLUSTER_ID NODE_ID`
读取归档，无 socket 启动新单 voter。它拒绝损坏归档、源共识身份及存在的目标目录。
恢复保留 PD 集群 ID 和 allocator/TSO 记录，删除旧租约附属键并提升 watch 压缩水位。
[归档实现](../../../pkg/storage/kv/native_backup.rs)、
[运维入口](../../../tools/pd-recover/bin_main.rs)。

`theme10_native_recovery_drill` 使用真实磁盘、实际 CLI 进程以及 Rust PD gRPC。
源节点仍可读时向隔离目标恢复。目标重开后核对六个持久键、独立共识身份、旧租约 fencing
和更高的实际 ID/TSO。源节点保留旧租约键。三次拒绝输入分别是校验失败、复用源身份、
重复恢复。另用 `theme10_native_backup_is_one_revision_during_writes` 在并发的两键
写中校验 12 份归档的 revision 与键值原子性。生产 RTO/RPO 暂缓。
[恢复测试](../../../tools/pd-recover/roundtrip_test.rs)、
[并发归档测试](../../../pkg/storage/kv/native_contract_test.rs)。

`cargo test --offline --locked -p astersql-pd-recover -- --nocapture` 退出码 0，
4 项通过。单次恢复 CLI 到新集群首个已核验读为 152 ms。该值是本地演练测量，
同配置重开仍使用独立冻结门槛。实际槽位 1，绝对 `CARGO_TARGET_DIR` 为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`。

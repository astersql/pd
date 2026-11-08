# 生产任务 38 的本地迁移与回滚回执

在隔离的官方 etcd 3.5.15 与单 voter SQLite/OpenRaft 上运行
`theme10_native_etcd_cutover_local`。源 PD 停止并释放监听后，用线性
`/pd/` 前缀读取取得一个修订版的完整测试快照。测试桥把快照写入隔离的
原生暂存目录，再调用原有原生归档与恢复 API。快照的租约项在暂存端使用
本地租约，恢复端移除这些项并使用新的共识身份。
[迁移用例](../../../cmd/pd-server/native_etcd_cutover_test.rs)、
[原生归档](../../../pkg/storage/kv/native_backup.rs)。

1. 修改已编码归档的租约水位并保留原摘要，恢复返回 `invalid native archive format or checksum`。
   目标目录未创建。随后用真实 etcd 比较事务模拟源端迟到写，修订版前进。
   重启源 etcd 并校验修订版和已确认的写入，再恢复源 PD 服务。
2. 再次停止源 PD 并取快照。恢复端读回全部无租约键的原始字节，拒绝旧
   租约比较，源修订版保持冻结。原生 PD 用原 PD 身份提供 GetMembers、
   AllocID、Tso 和 HTTP health，真实比较写入提交后关闭并重新打开原生目录，
   从 WAL 读回新值。
3. 回滚前，先校验该隔离夹具的全部持久变更恰为三键，分别为确认写、ID
   窗口和 TSO 窗口。注入过期修订版比较，真实 etcd 事务拒绝全部写入。
   以正确修订版一次提交三键，核对源持久键集合与目标一致，再启动源 PD。
   原生端已关闭，两个服务分时使用同一监听。

最终实测同一 PD cluster ID 为 `1794145293150691000`，ID 依次为
`1 → 1001 → 2001 → 3001`，TSO 物理毫秒为
`1794145293254 → 1794145296189 → 1794145299190 → 1794145302191`。
源端恢复服务后仍读到原生端确认的写入。

`cargo test --locked -p astersql-pd-cmd-pd-server --lib theme10_native_etcd_cutover -- --nocapture`
退出 0，1 通过、0 失败、0 忽略、22 过滤。槽位 1，
`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
测试使用进程内控制的写者和单修订版小快照。真实生产仍需要可审计的写者
冻结和授权栅栏、跨批一致快照与完整归档转换工具，以及生产回滚窗口验收。

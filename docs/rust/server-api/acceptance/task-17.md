# 第17项本地验收回执

**本地耐久性通过，写入性能未达标，第17项保留。** Rust 候选用例7通过，
覆盖多数派掉电、快照发布四处切断、磁盘故障、快照追赶和成员替换。
[掉电回执](../../production-meta/power-evidence.md)。

冻结的同机 etcd 负载为三节点、128笔两键事务和逐笔线性读。Rust release
候选三轮均恢复128笔，读 p99 与重启时间通过，写 p99 和吞吐未达到阈值。
恢复工具在外部 etcd 上通过真实备份往返，恢复后的 Rust PD 分配的 ID 和 TSO
均越过备份边界。[性能对照](../../production-meta/etcd-baseline.md)、
[恢复工具](../../production-meta/recovery-roundtrip.md)。

正式准入还缺任务21、29、30和37所要求的完整契约、并发历史、原生恢复及长稳。
本次 ADR 保留候选的实验状态，服务端覆盖清单仍按实际准入状态记载。
[ADR](../../production-meta/adr.md)。

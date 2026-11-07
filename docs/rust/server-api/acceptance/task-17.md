# 第17项本地验收回执

**本地耐久性和重放性能通过，正式第17项保留。** Rust 原候选用例7通过，
覆盖多数派掉电、快照发布四处切断、磁盘故障、快照追赶和成员替换。
[掉电回执](../../production-meta/power-evidence.md)。

冻结的同机 etcd 负载为三节点、128笔两键事务和逐笔线性读。Rust release
候选三轮均恢复128笔。持久应用表的写入未达标，保留完整同步的重放方案通过
所有冻结指标。并发两键历史与真实 etcd 同断言通过，12次进程中断后找回1536笔。
恢复工具在外部 etcd 上通过真实备份往返，恢复后的 Rust PD 分配的 ID 和 TSO
均越过备份边界。[重放证据](../../production-meta/replay-experiment.md)、
[恢复工具](../../production-meta/recovery-roundtrip.md)。

正式准入还缺任务21、29、30和37所要求的完整契约、并发历史、原生恢复及长稳。
本次 ADR 保留候选的实验状态，服务端覆盖清单仍按实际准入状态记载。
[ADR](../../production-meta/adr.md)。

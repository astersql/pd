# 第 17 项本地验收回执

**已通过耐久性、同机性能、跨后端契约、故障历史和原生恢复。长稳窗口运行中。**
外部 etcd 冻结阈值沿用既有证据。生产上线准入另行记录。
[ADR](../../production-meta/adr.md)、[长稳标识与验收方法](../../production-meta/soak.md)。

- 复用真实 SQLite 同步、快照和多数派掉电证据。原生路径三轮最差写入 p99
  为 18.673 ms，最慢读对 p99 为 0.642 ms，最低写入吞吐为 77.618 笔/秒，
  最慢重开首读为 62.014 ms。三轮均找回 128 笔、256 键。
  [掉电回执](../../production-meta/power-evidence.md)、
  [原生性能样本](../../production-meta/native-baseline.json)。
- 原生和真实 etcd 同跑 11 个共享用例及 5 个 revision/raw 扩展用例。
  三节点补充验证少数派拒绝线性操作、旧任期 lease fencing、快照追赶和重开。
  [跨后端回执](../../production-meta/task-21.md)。
- 32 轮随机分区及 SIGKILL 交替，找回全部 261 笔确认和 18 笔未知但已提交的写，
  并检查了 410 个成功线性读。 [故障历史](../../production-meta/task-29.md)。
- 使用实际 `pd-recover` 进程从检查过的原生归档恢复至新共识集群，重开后保留
  持久数据，旧 lease fenced，PD 的 ID 和 TSO 均越过备份边界。
  [原生恢复](../../production-meta/task-30.md)。

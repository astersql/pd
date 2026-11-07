# 原生元数据候选实验提案

状态为实验草案。首个候选是 OpenRaft 0.9.22 加 bundled SQLite。
原提案的 rusqlite 0.37.0 搭配 libsqlite3-sys 0.35.0 时使用 SQLite 3.50.2。
该版本处于上游公布的 WAL reset 并发损坏影响范围。Rust 实验接线使用
rusqlite 0.39.0 和 bundled SQLite 3.51.3，绑定与运行时版本均已核实。
当前裁决和复核条件见独立 ADR。
[OpenRaft 版本](https://docs.rs/crate/openraft/0.9.22)、
[rusqlite 版本及依赖](https://docs.rs/crate/rusqlite/0.37.0)、
[SQLite WAL reset 公告](https://sqlite.org/wal.html#the_wal_reset_bug)、
[SQLite 许可说明](https://www.sqlite.org/copyright.html)、[本轮 ADR](adr.md)。

| 路线 | 取舍与待证范围 |
| --- | --- |
| 嵌入 OpenRaft 和 SQLite | 复用共识、成员与快照接口。节点本地日志和状态仍须实现一致的崩溃边界。bundled C 依赖的目标平台构建仍需实测 |
| 窄语义自研共识或复用 raft-rs 低层接口 | 可以控制日志和执行路径，仍需自行负责 Ready 顺序、应用索引和快照传输。旧原型的快照缺陷给出了可复现的拒绝用例 |
| 外部 etcd 3.5.15 | 复用现有适配与真实契约对照。启动和持久化仍依赖外部服务，保留为过渡路线 |

生产任务3仅建立候选打开及真实重开验证。后续用同一候选分别检查以下边界。

- 串行化节点的日志、vote、截断和应用事务。SQLite 采用 `journal_mode=WAL`
  和 `synchronous=FULL`，macOS 额外启用 `fullfsync`。持久事务成功后才触发
  Raft 日志完成回调。持久应用表路径在成功前另同步应用事务。重放路径等待多数派
  持久日志和内存应用，重启依靠持久快照、日志重放和线性读屏障恢复派生状态。
  磁盘错误、提交结果未知或任期过期时停止确认。设计依据见
  [RaftLogStorage](https://docs.rs/openraft/0.9.22/openraft/storage/trait.RaftLogStorage.html)
  和 [SQLite PRAGMA](https://sqlite.org/pragma.html#pragma_synchronous)。
- 实验格式在数据库中记录 schema 版本、集群及节点身份。应用索引、成员视图、
  revision 和业务变更在同一事务提交。发布快照前校验内容、边界索引、身份及
  格式版本。截断日志之前验证快照已可读取，落后节点恢复必须核对 payload。
- 读屏障须证明当前领导权。watch 通过持久 revision 排序及显式压缩水位保留，
  在完整契约阶段对照边界、重放和取消。租约的超时判断由 leader 发起，
  撤销与删除通过复制状态机生效。重开及换主需要验证时钟回退、旧任期 fencing
  及租约状态，不能以观察到本机定时器触发替代集群行为。
- 掉电实验记录镜像隔离、flush/FUA、输入摘要、外部客户端确认和重开后的实盘
  对账。模型层已校验三个串行场景，单节点 SQLite VFS 已在两种引擎版本上
  验证真实 WAL I/O。后续需在 Rust 候选和共识确认链上复用这类故障对账。
  [模型校验](power-model-calibration.md)、[SQLite 实验回执](sqlite-power-evidence.md)。

任何已确认提交丢失、少数派确认线性读写、损坏静默通过、落后节点无法从
快照恢复或已移除成员仍可提交，均使该候选无法进入生产实现。备份恢复工具及
发布门依赖主题9实际交付。设备模型与 SQLite 实验均在 Darwin arm64 上执行。
本轮验收采用同机外部 etcd 实测阈值。部署平台、生产负载与 SLO 仍暂缓。
[本轮基线和阈值](etcd-baseline.md)。
[旧原型裁决](../adr-native-meta-store.md)、[当前准入](task-01.md)。

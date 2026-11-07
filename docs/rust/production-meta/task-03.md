# 实验候选磁盘入口与多数派掉电对账

基于 `3f2c4079b`。Rust 候选为 OpenRaft 0.9.22 与 rusqlite 0.39.0，bundled
SQLite 运行时实测 3.51.3。候选仅在 `storage-kv` 测试目标中注册。
[数据库入口](../../../pkg/storage/kv/candidate.rs)、
[OpenRaft 存储适配](../../../pkg/storage/kv/candidate_raft.rs)。

OpenRaft 日志持久回调等到 SQLite 事务提交才返回成功。应用事务一起保存两条
业务记录、revision、应用索引和成员视图（若该批包括成员日志）。落盘使用
WAL 和 FULL 同步。VFS 实验在每个节点的专用临时镜像上截获真实 SQLite I/O，
在 `xSync` 前仅更新易失视图。切断后拒绝原设备 I/O，恢复只从复制出的同步镜像
开始。实验通过独占锁限制每盘只有一个连接，确认日志独立写入并同步。
[VFS](../../../pkg/storage/kv/candidate_power_vfs_test.rs)。

测试建立三个真实 OpenRaft voter 和三个 SQLite 数据库。隔离一个 follower 后
发出两键事务，客户端收到 revision 1 才写确认日志。切断后核验原 leader 和
参与提交的 follower 镜像都有该日志，隔离节点镜像没有。弃用原 leader 镜像，
在其余两个节点重开，新的 leader 通过线性读屏障并找回确认日志中的两条记录。
[多数派用例](../../../pkg/storage/kv/candidate_raft_test.rs)。

| 命令 | 退出码与结果 |
| --- | --- |
| `cargo test --offline -p astersql-pd-storage-kv --lib theme10_candidate_open -- --nocapture` | 101，1个行为红灯，SQLite 报 `no such table: state` |
| `cargo test --offline --locked -p astersql-pd-storage-kv --lib theme10_candidate_open -- --nocapture` | 0，1通过，建库、关闭及重开读取同一记录 |
| `cargo test --offline --locked -p astersql-pd-storage-kv --lib theme10_candidate_ -- --nocapture` | 0，3通过，包含普通重开和多数派掉电恢复 |

动态领取槽位1，`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`。
本项验证了一个被确认的多数派提交及两个键的恢复。模型在操作边界截断，
没有穷举扇区撕裂、快照安装、动态成员变更和网络传输。生产原生 MetaStore
还需要完整事务、watch、lease 等契约。

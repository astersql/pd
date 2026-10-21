# ADR: 原生元数据引擎候选验证

日期：2026-10-21。状态：**淘汰本次候选组合，不进入生产实现**。

## 决策与边界

可丢弃原型位于 `pkg/storage/raft-prototype`：真实 `raft-rs` 三节点共识、
确定性内存传输、真实磁盘全量日志镜像。它验证 CAS、revision、事件回放、
ReadIndex、成员宕机和恢复；不接入 `pd-server`。`prototype` feature 默认关闭，
根 facade/runtime 无原型依赖，不改变当前默认后端。

淘汰的是 **MemStorage + 自制全量镜像适配**，不是 Raft 算法或 raft-rs。
可复现的硬失败：已落后节点重连时，已恢复并截断日志的 leader 使用
MemStorage 提供网络快照，快照没有业务状态数据。原型明确返回 Unsupported，
拒绝应用空快照，不能假装恢复成功。`rejection_gate_lagging_member_needs_real_snapshot_storage`
只有复现此拒绝才通过；它不是网络快照恢复成功的证据。

本任务允许以有证据淘汰结束，因此在该门槛失败后停止推进生产 MetaStore adapter。
**原型未实现 MetaStore trait**；只有单 key CAS/最新线性读/完整事件历史探针，
没有 lease、TTL、删除、多 key 原子事务、历史读、prefix watch 流或 compaction 契约。
存储库的既有契约回归不能算作原型通过完整 MetaStore 契约。

## 预先门槛和候选比较

硬门槛：已确认写入在多数派、leader 宕机、进程退出和本地快照恢复后仍存在；
少数派不能确认写入/线性读；CAS 冲突不消耗 revision；事件 revision 连续；
损坏不得静默跳过；落后成员必须能从快照恢复。任一失败不得进入生产。
生产还要求 lease/fencing、压缩水位、随机并发历史检查、磁盘故障与跨平台证据。

| 候选 | crash/fsync 边界 | snapshot/compaction 与恢复 | 构建与结论 |
| --- | --- | --- | --- |
| 本次自制全量镜像 | 每次 Ready 将 snapshot、entries、HardState 写 pending，SHA-256，sync_all，原子 rename，再同步父目录；之后才发送确认 | 本地恢复可验证；网络快照丢 payload；所有事件无限保留，每轮重写完整日志 | 本机 macOS arm64 实测；**淘汰**，仅保留实验 |
| SQLite WAL | 候选配置 WAL + synchronous=FULL，事务提交包含 WAL 同步；本次未运行 | 需统一提交 Raft HardState/log、状态机 applied 和快照元数据，设计 checkpoint、备份及 watch 保留 | 后续优先原型候选，尚未选择为生产引擎；需验证原生依赖与各平台构建 |
| RocksDB | 必须显式同步 WAL，验证 write batch/崩溃边界；本次未运行 | 需验证 checkpoint、列族、压缩与恢复时间，不以 API 便利性判断 | C++ 工具链和跨平台发布成本需单独测量；本次不引入 |

SQLite 的同步语义依据 [官方 PRAGMA 文档](https://www.sqlite.org/pragma.html#pragma_synchronous)，
Raft Ready 的保存/发送/推进顺序依据 [RawNode 文档](https://docs.rs/raft/0.7.0/raft/raw_node/struct.RawNode.html)。
候选表是设计比较，不宣称 SQLite/RocksDB 已通过本仓库 durability 或性能测试。

## 依赖与持久化配置

- raft/raft-proto `0.7.0`，发布包 `.cargo_vcs_info.json` 均指向
  `10c6e9db6792b85c81784e44fc278f895d5f0ab0`；Cargo.lock 固定 registry 校验和。
- protobuf `2.28.0`、slog `2.8.2`，workspace 集中固定版本；仅原型 feature 引入。
- 三个固定 voter（1、2、3），check_quorum/pre_vote 开启，election_tick=10、
  heartbeat_tick=1；测试传输由调用方推进，不对应真实时间网络。
- 每节点独立 `.wal` 文件，SHA-256 + JSON 编码 protobuf 数据，日志和快照均落真实临时磁盘。
  未发布的 `.pending` 被忽略；已发布镜像损坏返回 InvalidData，不尝试猜测恢复。
- 真正进程级恢复测试启动独立测试子进程，承载三个节点，提交确认后由父进程 kill/wait，
  再从同一磁盘创建新集群。不是三台机器独立断电，也不是磁盘缓存断电证明。
- 老版 protobuf-build 拒绝本机 protoc 36，且无 macOS arm64 bundled fallback；
  显式使用 `PROTOC=/opt/homebrew/opt/protobuf@21/bin/protoc`，实测 `libprotoc 3.21.12`。
  Linux/Windows 和断电持久性未验证，不能据本机结果承诺跨平台。

## 故障矩阵与验证范围

| 场景 | 检查内容 | 结果 |
| --- | --- | --- |
| 顺序历史 | 20 次成功 CAS + 线性读、20 次失败 compare；3 节点事件 1..20 | 通过；这是有界顺序历史断言，不是并发 Jepsen 检查 |
| leader 隔离/宕机 | 隔离节点读/写拒绝，多数派选新 leader 并提交，重启旧节点替换未提交冲突后缀 | 通过 |
| 本地快照恢复 | 三节点快照重开，再写入，事件 1..11 连续 | 通过 |
| 进程 kill/restart | 确认提交后 kill 子进程，恢复值/revision，再提交并验证事件 1,2 | 通过 |
| 数据损坏 | 发布镜像 bit flip 和截断 | 均明确拒绝，无静默丢数据 |
| 快照发布中断模型 | 留下不完整 pending，继续读取旧正式镜像 | 通过；未覆盖 rename/fsync 系统调用中途断电 |
| 落后节点 + 截断日志 | 重启快照节点后重连旧成员 | **淘汰门槛失败**：业务 payload 缺失，明确拒绝 |

恢复计时、测试数量及确切命令见下方验证记录。测试规模只适于正确性探索，
耗时包含真实同步写盘、重新选主和测试并发干扰，不能当作性能 SLO。
原型未验证 IO 错误后继续服务；任一 IO/快照错误后必须丢弃该 Cluster，不能继续调用。

## 后续拆分

1. 设计真实 Storage adapter，显式持久化 term/vote/commit/log 和 snapshot payload，
   实现可重传快照及已落后成员恢复，加入故障门槛的正向验收。
2. 分别比较 SQLite/RocksDB 的 fsync、checkpoint、压缩、写放大和恢复分位数；
   注入 ENOSPC、短写、fsync/rename 失败、进程中断和机器断电，证明已确认提交不丢失。
3. 在引擎门槛通过后实现完整 MetaStore trait，复用同一套 etcd oracle 契约，
   包括 lease、fencing、revision 历史、watch 压缩和取消，不能用探针替代契约。
4. 增加并发线性历史检查、随机网络/进程故障种子、动态 membership、长时间 soak
   与 Linux/macOS/Windows 构建。生产化另建计划；本 ADR 不授权切换默认后端。

## 验证记录

共享槽位：1。`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
所有 Cargo 调用在原子领取槽位后执行，阶段结束释放自己的锁并保留缓存。

首次红灯：计划中的无 feature 命令退出 101（target 要求 prototype feature），
该结果仅证明 feature 隔离，不算业务红灯。启用 feature 后，六个行为测试因空日志
读取崩溃而失败（1 helper 通过），随后修复空日志与独立保存快照 payload 的恢复路径。
另复现过多数派选举尚未完成时写入拒绝，修正测试等待真实 tick 推进选举。

所有命令均继承上述 CARGO_TARGET_DIR 和 PROTOC。

| 命令 | 退出码 | 有效数量/结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | Rust 格式化 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace 可解析 |
| `cargo fmt --all -- --check` | 0 | 无格式差异 |
| `cargo check --workspace --locked` | 0 | 默认构建不启用原型 |
| `cargo test --locked -p astersql-pd-raft-prototype --all-features --test cluster_test -- --nocapture` | 0 | 8 harness tests：6 正向故障/行为场景 + 1 淘汰证据 + 1 子进程入口；7 个有效场景 |
| `cargo test --locked -p astersql-pd-storage --lib meta_store_test -- --nocapture` | 0 | 9 既有契约测试通过 |
| `cargo test --workspace --locked` | 0 | 68 通过，1 忽略（既有 TSO 双进程测试，要求独立兼容脚本）；默认不运行 prototype 集成测试 |
| `cargo test --workspace --locked -- --list` | 0 | 69 项登记测试，核对上述总数；此命令本身不算行为验证 |

最后一次原型运行共 6.00 秒。本地三节点 snapshot restore + 选主/读/追加验证
403,418 µs；SIGKILL 后的重建 + 选主/读/追加验证 1,091,498 µs。
这些为单次本机测量，不是纯磁盘恢复时间或性能上限。
原型最后一次输出包含 `REJECT native-backend candidate`，故本任务的完成结果是
**验证完成并淘汰候选**，不是原生 MetaStore 或生产 durability 已完成。

# 多数派日志重放与写入性能复核

基于 `a0a6ca5f8` 的等口径测量复跑。保持冻结的 etcd 阈值、三节点、同机
Darwin arm64/APFS、128笔两键事务、逐笔线性读和重开对账。SQLite 使用 WAL、
`synchronous=FULL` 和 `fullfsync=ON`。基准的候选节点为隔离的独立进程。
[冻结基线](etcd-baseline.md)。

## 写入分段诊断

探针委托原始 SQLite VFS 的所有 I/O，仅在实际 `xSync` 入口计时并区分 WAL
与数据库文件。Rust 记录请求、日志追加、非空复制 RPC 和应用阶段时间戳。
三个配置各跑三轮。阶段可并行，表中数值不能相加得到请求时间。

| 配置 | 主节点每笔 WAL 同步 | 日志追加中位数 | 复制 RPC 中位数 | 应用中位数 | HTTP 写中位数 |
| --- | --- | --- | --- | --- | --- |
| 持久化应用表 | 384 笔均为 2 次 | 7.523 ms | 6.395 ms | 9.541 ms | 24.728 ms |
| 持久化应用表，诊断性关闭自动 checkpoint | 384 笔均为 2 次 | 7.331 ms | 8.453 ms | 9.812 ms | 26.497 ms |
| 重放 | 384 笔均为 1 次 | 5.189 ms | 5.820 ms | 0.001 ms | 13.304 ms |

原来的 613.636 ms 是无探针轮次的 p99，无法追溯单笔调用。探针复现的
652.159 ms HTTP 长尾对应日志37。主节点该笔追加和应用阶段耗时分别为
250.749 ms 和 265.200 ms，其中两次原生 WAL 同步为 132.780 ms 和
131.822 ms。同期跟随节点的 WAL 同步也超过 130 ms。主节点同一笔复制 RPC
记录至多 12.157 ms。三种配置在测量写窗口内均未观察到数据库文件同步。
复现长尾的第三轮中，客户端写 p99 为 319.190 ms，Rust HTTP 处理器为
318.898 ms。同轮两组 p99 相近，连接和处理器前排队没有解释这次差距。
这表明复现的长尾主要与 WAL 同步及阶段间的等待相伴。数据不能将余量归因
到某个调度器或具体设备，也不支持把长尾归因于定时 checkpoint 或连接排队。
[探针](../../../pkg/storage/kv/candidate_profile_test.rs)、[原始时间线](replay-diagnostics.json.gz)。

## 重放与恢复边界

候选保留多数派日志、投票、快照和截断的完整 SQLite 同步。应用表是从已提交
日志导出的内存视图。重开只加载最近的持久快照，集群确认领导权并应用当前任期
的读屏障后才向客户端读出视图。单节点即使已载入快照也不能读。日志条目用
base64 编码二进制键值，解码仍接收原实验的字节数组格式。
[OpenRaft 提交水位契约](https://docs.rs/openraft/0.9.22/openraft/storage/trait.RaftLogStorage.html#method.save_committed)、
[OpenRaft 线性读](https://docs.rs/openraft/0.9.22/openraft/raft/struct.Raft.html#method.ensure_linearizable)、
[重放实现](../../../pkg/storage/kv/candidate_replay.rs)。

快照继续保存版本、校验摘要、应用索引、成员和数据。安装在状态机工作器中执行，
OpenRaft core 可能先进入 purge，因此候选在真实持久快照覆盖目标索引前禁止
物理删除。测试先用同步镜像重开到旧快照，再恢复有确认日志的后缀，随后构造
并截断快照，验证落后 learner 安装快照和成员替换后仍能提交。保留原四个
快照发布中断用例。原先只检查磁盘快照并立即拒绝 purge 的尝试使新 learner
退出，现由持久化完成通知协调。重放没有关闭 SQLite 自动 checkpoint。

## 同口径结果与附加验证

| 比较项 | 冻结阈值 | 重放无探针三轮的保守值 |
| --- | --- | --- |
| 写 p99 | ≤ 37.879 ms | 19.712 ms |
| 两次线性读合计 p99 | ≤ 16.618 ms | 0.838 ms |
| 写吞吐 | ≥ 73.580事务/秒 | 76.757事务/秒 |
| 重开至正确读 | ≤ 418.032 ms | 62.433 ms |
| 恢复 | 每轮128笔 | 三轮各128笔，丢失0 |

基准仍使用原客户端脚本，增加 `--candidate <release 测试二进制> --replay`。
探针使用额外参数 `--profile-output <gzip 路径>`，仅诊断轮次使用 `--wal-pages 0`。
编译命令和动态槽位规则见基线回执。本轮实际槽位1，构建和三轮均退出0。
重放初版最低吞吐为73.560事务/秒，略低于冻结阈值。最终结果包含紧凑日志编码。
[无探针原始结果](replay-performance.json)。

`cargo test --offline --locked -p astersql-pd-storage-kv --lib theme10_candidate_ -- --nocapture --test-threads=2`
退出0，9通过。并发历史采用4个客户端，每侧32写32读，针对候选和真实 etcd
使用同样的两键原子检查、revision 连续性与实时先后约束。这是两键实验接口
的差分，范围之外的 watch、lease、事务分支等仍由完整 MetaStore 契约负责。
[专项输出](replay-tests.txt.gz)、[并发验证](../../../pkg/storage/kv/candidate_history_test.rs)。

`cargo test --offline --locked -p astersql-pd-storage-kv -- --test-threads=2`
退出0，57通过，0失败，2个子进程入口按设计忽略。

`python3 -B docs/rust/production-meta/check_candidate_replay_cycles.py <release 测试二进制>`
退出0。32秒内对私有 fixture 进程执行12次主节点中断、重新选主和旧节点重开，
最后全停重启并找回确认日志中的1536笔事务。该短跑是进程故障正确性验证，
掉电证据来自独立 VFS 用例。正式长稳仍需原生产任务37的时长、负载及运行回执。
[重复恢复输出](replay-cycles.json)。

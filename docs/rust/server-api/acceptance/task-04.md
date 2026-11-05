# 任务04：键空间GCv2完整验收

2026-11-06，基准e7e39e066。当前四方法验收完成，取代[历史局部记录](task-04-partial.md)的未完成结论。[原始完整回执](task-04.json.gz)包含全部命令、失败/通过日志、请求响应、输入及源码/二进制SHA、实际物理记录、TSO和清理状态。

依赖43–47及132的原有真实GC manager；缺少的48–53已按原拥有任务推进并分别提交：48 `fb7edafbf`、49 `206f977df`、50 `7352b7cde`、51 `66f90c34b`、52 `1aecad976`、53 `e7e39e066`。对应[48](../../control-plane-mcs/task-048.md)、[49](../../control-plane-mcs/task-049.md)、[50](../../control-plane-mcs/task-050.md)、[51](../../control-plane-mcs/task-051.md)、[52](../../control-plane-mcs/task-052.md)、[53](../../control-plane-mcs/task-053.md)保存领域失败、真实Go同源验证和持久化/生命周期证据。

本次仅补GetAllGCSafePointV2的Grpc/SyncGrpc适配，调用同一任期manager的get_all_keyspaces_gc_states(true)，排除Null、保留启用统一GC空间零状态与revision=0；领域错误返回UNKNOWN响应header及空列表。三个单空间方法已有接线继续复用，未在transport拼装存储事务。现代GC状态RPC属于外层任务8；两个废弃watch保留task037精确Unimplemented契约。

完整方法矩阵（每实现62项同输入观察，44组原始证据）：

| 方法 | 成功与参数/管理类型 | follower与取消 | 实际故障与WAL |
| --- | --- | --- | --- |
| GetGCSafePointV2 | 空间2/3独立值、disabled/archived读取、Null别名；统一GC/缺失空间错误；absent/V3 oneof按旧getter落0 | 未bootstrap header、错cluster精确FailedPrecondition、真实第二节点Unavailable/not leader、SIGSTOP前入场deadline | Null hex持久值损坏的完整Unknown消息；两侧fresh连接从同WAL恢复2/3及完整entries |
| UpdateGCSafePointV2 | 20/30隔离推进、同值事务、回退无写、超txn拒绝、统一/缺失空间错误、Null别名、absent/V3 | 相同四类入场边界；真实TCP响应扣留期间，独立etcd读取确认GC21已提交，再观察客户端Cancelled/Timeout expired | 空值但存在的revision触发真实etcd CAS冲突，空间2及Null均无部分写；成功counter+1与GC点同mod_revision；同WAL恢复20/30 |
| UpdateServiceSafePointV2 | 实际独立屏障、低于txn忽略、UTF-8名称、删除、非法worker TTL、空ID、统一GC重定向、local/global native_br；60秒/1秒实际TSO TTL、向上取整到秒、真实到期清理 | 相同四类入场边界；独立确认lost_reply屏障提交后，扣留响应直到客户端deadline | 请求能进入gRPC，但真实etcd事务超过max-request-bytes，精确Unknown/Internal消息且entries不变；WAL恢复空间3无限屏障 |
| GetAllGCSafePointV2 | 启用独立及统一GC空间、Null排除、disabled/archived排除、列表revision=0；恢复后包含修复的空间7 | 相同四类入场边界，取消测试使用真实只读调用 | 冷空间7的坏decimal txn点使实际读取失败，UNKNOWN在header、无部分列表、无持久写；同WAL恢复列表及metadata过滤 |

“入场边界”覆盖四方法各自的未bootstrap、错cluster、follower、取消请求。客户端超时不等于回滚；两个写方法分别有真实已提交但未收到响应的正向证据。Get为只读单键路径，不捏造不存在的写冲突；列表的适用读取故障使用真实坏值及事务读取。所有故障保留完整状态与revision对照。

比较只消除独立cluster ID、Go无序列表顺序及两侧独立TSO绝对时间差。原始header/code/完整message、真实key/bytes/header/create/mod revision、TSO前后界及绝对expiration均保留。失败/无写和WAL恢复在各实现内部逐条精确比较；跨实现不抹掉持久bytes或RPC revision。

验证：

- 列表red：`cargo test --locked -p astersql-pd-compat-tests --lib sa_004_list_admission -- --nocapture`，1 failed，exit101，真实Rust返回Unimplemented/Not yet implemented。早先Go TTL断言及测试顺序导致的多数派丢失均独立保留，未计为实现red。
- `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`：exit0；二进制SHA在回执。
- `cargo test --locked -p astersql-pd-compat-tests --lib sa_004_ -- --nocapture`：2 passed、0 failed、0 ignored；含完整双实现矩阵和Rust列表准入。
- `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`：2 passed，1既有ignored；忽略项不计证据。
- `cargo test --locked -p astersql-pd-server --lib task_037_ -- --nocapture`：2 passed，继续核对废弃watch精确消息和剩余stub清单。
- `cargo test --locked -p astersql-pd-runtime --lib mcs_132_ -- --nocapture`：2 passed，保持唯一任期manager及真实fence。
- `cargo test --locked -p astersql-pd-compat-tests --lib sa_003_ -- --nocapture`：1 passed，旧GC三方法回归。
- metadata、fmt check、workspace check及git diff --check：exit0。完整workspace test按外层任务24集中执行，本项不声称已执行。
- 每个Cargo阶段动态原子领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；所属脚本和子进程结束后释放自己的锁，保留缓存。
- 固定Go基准SHA364477a0…3147，kvproto f8ad079c8e9812ed5900b0c12b5f1d97171464c4；SDK/协议zip/官方etcd3.5.15及etcdctl/PROTOC指纹准入来自任务1并在回执重新核对。Go/模块1101项存在性及SHA首尾一致，未运行Go写入、make、failpoint注入或生成/整理。所有plan.md与来源史诗只读。

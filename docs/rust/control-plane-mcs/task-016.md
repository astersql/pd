# 任务016：恢复请求与初始阶段

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

新canonical crate `astersql-pd-unsaferecovery`只依赖core、errs和async-trait。
领域控制器采用已有StoreInfo的真实生命周期与断连判定，不包含protobuf、传输或
私有runtime。时钟由调用者传入，跨任期共享的RecoverySteps由节点宿主持有。

正在运行的恢复优先拒绝新请求；显式模式先验证所有ID，再逐个持久化tombstone。
空列表、不存在的store、仍连接的Preparing/Serving store拒绝且不会先埋葬有效ID。
严格大于20秒才算断连。自动模式跳过埋葬，但仍排除传入failed集合、Removed及
physically destroyed store。成功才发布CollectReport、初始step、deadline和预期报告集合；
正值执行超时覆盖默认60秒，其他值沿用默认，并保存paranoid选项。
保留直接领域调用中Go的int64 duration秒数乘法回绕；HTTP按原API范围验证秒数。

生产HTTP入口复用Go JSON数字转换和对象解析；别名重复、类型及范围错误保持原顺序。
命令在已有term coordinator串行执行，使用真实fenced StorageEndpoint提交，再更新
canonical store cache、既有store-limit清理、规则cache和rolling统计。
二次清理失败不撤销成功tombstone，保留诊断。控制器在共享mutex内，取消准入future
不会因临时取出控制器而丢失控制器。恢复暂停标志独立于用户halt-scheduling配置，
现有scheduler/checker决策及AskSplit读取该标志。

## 当前证据

[完整日志、确切命令、退出码、来源SHA](task-016-evidence.json.gz)。初始red编译后在
接受请求仍停留Idle的行为断言失败。补齐阶段转换后领域测试通过；后续共享控制器
接线的编译错误已保留并修复。最终验证如下：

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-unsaferecovery --lib mcs_016_ -- --nocapture | 0 | 4聚焦通过 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_016_ -- --nocapture | 0 | 1聚焦通过，两种真实Go/Rust模式 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_015_ -- --nocapture | 0 | 3相邻通过 |
| cargo test --locked -p astersql-pd-runtime --lib schedule_worker_test:: -- --nocapture | 0 | 7相邻通过；既有1 ignored不计通过 |
| cargo test --locked -p astersql-pd-runtime --lib grpc_ask_split_fault_test:: -- --nocapture | 0 | 1相邻通过 |
| cargo test --locked -p astersql-pd-runtime --lib member_role_worker_test::task_007 -- --nocapture | 0 | 2相邻通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 77 packages |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo tree --locked -p astersql-pd-unsaferecovery -e normal | 0 | 无protobuf、etcd-client和server普通依赖 |

去重5聚焦+13相邻通过。动态槽位1，绝对CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；自有进程结束后释放锁，保留缓存。
Go/模块1101项存在性、SHA、Git状态与起始保护基线一致。

已有只读Go oracle的显式/自动模式各比较14个精确HTTP状态和响应体，包括负数与
小数ID的Go转换、非法列表、在线/缺失store、超时/别名错误、成功和重复请求。
双方存储生命周期及AskSplit暂停错误逐字相等。读取Go真实etcd记录证明tombstone
已持久化；Go show证明初始CollectReport及预期存活store数。Rust节点对应检查同一
canonical控制器和实际持久记录。随机初始step不跨进程比较，分别证明非零及宿主
共享step源的单调性；不掩盖事务revision或错误文本。

原生etcd真正停止后提交失败，阶段仍Idle、step未分配、cache未变且未暂停调度；
重启同一WAL并确认旧连接恢复后，重试成功持久化tombstone并进入CollectReport。
没有Go构建、修改或failpoint注入。

本项只证明准入和初始阶段；心跳报告、Region分析、各类恢复计划、重试及终止仍属于
后续17–24任务。完整包级/workspace测试依用户授权统一执行，保留任务待回归。

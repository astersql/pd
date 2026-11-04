# 任务021：TiFlash清理候选

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

先检查最新有效Region是否失去完整quorum，再按既有raft新鲜度选择副本；
仅选中TiFlash write store时生成该store上该Region的tombstone。
TiKV副本更新、同等新鲜度优先TiKV、已有健康quorum、其他engine或失败状态均不
生成这种清理。它不是扫描并删除所有TiFlash副本，也不改动原报告或TiKV成员。

清理在ForceLeader和Demote准备之前，匹配Go生成器优先级。受影响范围使用区间
索引的最新epoch元数据，与ForceLeader记录所选leader范围的规则不同。
TiFlash报告清除后，仍在成员表中的旧副本作为orphan供后续force/demote处理，
保留其Learner角色。缺失内部选举候选返回安全错误。

真实term由相同报告集合和store registry准备候选，生产适配层转换protobuf；
分发和阶段step仍待任务23，未把完整恢复流程判为完成。

## 验证

[完整命令、退出、原始日志与来源SHA](task-021-evidence.json.gz)。red编译后在
必须先清理较新TiFlash副本却没有实际tombstone的行为断言失败。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-unsaferecovery --lib mcs_0 -- --nocapture | 0 | 28通过：5本项+23前置相邻 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_021_ -- --nocapture | 0 | 1本项，两种真实Go/Rust新鲜度场景 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_020_ -- --nocapture | 0 | 1相邻，四种实际降级场景 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_019_ -- --nocapture | 0 | 1相邻，三种实际ForceLeader场景 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 77 packages |

去重6聚焦+25相邻通过，0失败、0忽略。动态槽位1，CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；自有进程结束后释放锁、保留缓存。
Go/模块1101项存在性、SHA及Git状态保持一致。

已有Go二进制和Rust节点都通过正常PutStore注册TiFlash，再接收相同真实心跳报告。
TiFlash新鲜度更高时仅store3收到region7 tombstone；TiKV更新时不清理TiFlash，
改为TiKV上的ForceLeader候选。每种三个store的creates、demotes、tombstones和
force_leader逐字段相同，TiKV原报告及成员保留。Go实际阶段分别为TiFlash清理或
ForceLeader，原始step保存并检查+1。Rust候选step=0等待任务23，不伪装成已分发。

后续orphan的force/demote链由领域快照用例验证，未冒称已执行完整多轮Rust运行期。
完整包级/workspace回归依用户授权统一执行，保留待回归任务文件。

# 任务017：恢复心跳报告收集

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

控制器按预期store集合保存当前step的完整恢复领域投影：Region元数据、raft的
term/vote/commit/last index、applied index、force-leader及commit-merge标志。
无protobuf依赖；其他存储引擎本地字段留在适配层，不参与恢复算法。

首条当前step报告增加一次计数；重复报告替换为最新内容，不再次计数，也不再次
触发“全部首次收齐”。迟到/未来step、缺失报告、未知store和非运行阶段不计数。
已失败store的直接领域调用返回Go原错误。初始CollectReport握手向未报告store发送
带当前step的空计划；已请求但未到重试deadline不重复发送，到deadline当刻仍不重发，
严格超过才重试，已报告和未知store不发送。其他阶段的计划分发仍属于任务23。

gRPC通过实际RoleServices转发至同一term控制器。带StoreReport的心跳验证store存在
及tombstone状态，然后绕过普通统计更新；普通心跳继续更新原统计，再处理收集握手。
未配置恢复能力的其他StoreHeartbeatService保留原Unimplemented边界。
没有新worker、私有runtime、全局状态或Go写入。

## 验证

[全部命令、退出码、原始日志及来源SHA](task-017-evidence.json.gz)。red编译后在
当前step报告未增加计数的断言失败。首次宿主差分揭示RoleServices仍调用默认能力，
返回Unimplemented；补齐真实转发后通过。最终加强每条恢复报告携带不同容量值，
再次证明统计旁路，避免相同输入掩盖错误更新。

| 命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-unsaferecovery --lib mcs_01 -- --nocapture | 0 | 7通过：3本项+4准入相邻 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_017_ -- --nocapture | 0 | 1本项通过；加强统计输入后再次通过 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_016_ -- --nocapture | 0 | 1相邻通过 |
| cargo test --locked -p astersql-pd-server --lib store_heartbeat_adapter_test:: -- --nocapture | 0 | 7相邻通过 |
| cargo test --locked -p astersql-pd-runtime --lib wire_store_heartbeat_test:: -- --nocapture | 0 | 1相邻通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 77 packages |

去重4聚焦+13相邻通过，0失败、0忽略。动态槽位1，CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；自有进程结束并释放锁，保留缓存。
1101个Go/模块文件的存在性、SHA、Git状态与保护基线一致。

真实Go PD及Rust节点以相同bootstrap/store/心跳序列执行：idle报告和当前恢复报告
都不改写容量；重复、旧step、未来step、缺失报告以及初次请求的8组观察逐项相同。
双方各自原始step写入日志，分别断言后续计划精确回显，跨独立进程不要求随机初始
step相同。Go show实际停留CollectReport的2/3；Rust canonical控制器有相同缺失store，
并验证重复报告保留新的epoch和完整raft新鲜度字段。没有归一化容量、计数或错误。

收齐事件由领域方法返回ready；后续Region索引、计划生成和阶段推进属于18–24。
本项不把尚未接入的完整恢复流程判为完成。完整包级/workspace回归依用户授权统一执行。

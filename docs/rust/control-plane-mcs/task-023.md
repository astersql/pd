# 任务023：阶段分发与确认推进

状态：已完成，待统一包级与workspace回归。任务102尚未完成。

Controller拥有当前阶段计划和报告确认；所有成员须报告当前step才能推进，阶段切换清空
报告、递增共享step。重发仅在严格超过deadline后发生，payload和step保持不变；已确认、
未知store及终态不再下发。保留Go阶段顺序与Demote无工作时的一次前置阶段重查。
ExitForceLeader以空计划要求退出，确认后才结束；调度halt由实际controller阶段发布。

累积受影响范围及新建Region，生产HTTP提供稳定Show历史、动作摘要和当前分发状态。
仅执行过操作阶段的Finished清空canonical Region缓存；CollectReport直接完成保留缓存。
生成器候选仍是未分配step的快照；真实心跳响应使用controller分配的阶段step和完整计划。

完整响应差分发现两个实际接线缺口并修复：恢复报告跳过普通统计时响应state保持默认；
cluster_version读取当前持久配置。旧store未声明version按Go ParseVersion的Base1.0.0
参与canonical最小store版本更新和持久化，没有在比较时忽略版本字段。

## 验证

[全部阶段命令、退出、日志、来源SHA](task-023-evidence.json.gz)。编译后的首次red在
非Collect阶段未下发step的断言失败。首次完整响应比较另发现旧store版本0.0.0/1.0.0
差异，修复后通过。离线完整metadata遇到未缓存跨平台crate并失败，随后用本平台
canonical crate check更新单条workspace依赖；未把失败或零测试记为通过。

| 最终命令 | 退出 | 有效结果 |
| --- | --- | --- |
| cargo test --locked -p astersql-pd-unsaferecovery --lib mcs_0 -- --nocapture | 0 | 38通过：4本项+34相邻 |
| cargo test --locked -p astersql-pd-runtime --lib mcs_0 -- --nocapture | 0 | 11通过：1本项+10相邻 |
| cargo test --locked -p astersql-pd-runtime --lib grpc_ask_split_differential_test::readonly_ask_split_differential -- --ignored --exact --nocapture | 0 | 1相邻，实际Go/Rust split差分 |
| cargo test --locked -p astersql-pd-runtime --lib split_version_test:: -- --nocapture | 0 | 1相邻版本边界 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过 |
| cargo metadata --format-version 1 --no-deps | 0 | 通过 |

去重5聚焦+46相邻通过；完整包级/workspace测试依用户授权统一执行。
动态槽位1，绝对CARGO_TARGET_DIR为
`/Users/xiangmin/worker/code/pd/target/rust-slot-1`；结束释放自有锁、保留缓存。
Go及模块1101项存在性、SHA及Git状态保持一致。

已有只读Go二进制与真实Rust/etcd节点分别执行健康报告直接完成，以及报告驱动的
ForceLeader→Demote→ExitForceLeader→Finished多轮流程。外部store报告按收到的真实
计划更新；未宣称启动实际TiKV进程。完整心跳帧与Show历史一致，包括空force字段、
失败peer身份、动作诊断、cluster_version、state和重发。仅对分别核验的运行cluster身份、
逐次+1且原始值留档的step起点、墙钟、Go报告状态无序ID列表做比较投影。
通过真实gRPC查询验证缓存是否保留或清空；完成后halt解除且Show稳定。

错误、主动abort及整体超时后的best-effort退出仍属任务024，尚未标记完成。

后续更新：上述分发/错误退出限制已由[任务024](task-024.md)的当前完整心跳帧及多轮差分补齐。

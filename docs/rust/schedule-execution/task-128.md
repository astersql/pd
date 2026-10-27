# 任务128：覆盖区域清理桥接

前置127的提交 `34ac34381` 与 [task-127.md](task-127.md) 已核实：canonical
cluster-utils、HotCache/witness 桥接及 StatePipeline 已有真实行为证据，workspace
1823项通过。本任务直接复用现有 heartbeat effect，不新增第二条管线。
整体和包级 plan.md 保持只读。

用户后续明确允许以只读 Go 源码/测试推导 + Rust 真实行为/故障验证替代缺失的
Go 运行 oracle。`oracle-index.json` 将 HandleOverlaps 标为 source_assertion，
未冻结 Go 运行输出；本任务未运行 Go 测试、failpoint 或生成器，也不声称运行差分。
[provenance](logs/task_128/provenance.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、5份来源哈希及 fixture 哈希；
这些 Go 文件均与来源 commit 字节相同，Go binary/output 哈希均为 null。
`pkg/cluster/cluster_test.go` 只有两个热点 benchmark，不覆盖 HandleOverlaps。

源码依据：HandleOverlaps 每个区域开始前检查取消；可选 RegionStatistics 立即
ClearDefunctRegion，可选 affinity InvalidCache，label MarkDefunctRegion，最后
placement InvalidCache。region_collection.go 证明 label 在 ClearDefunctRegions
才扣减且重复清理幂等；affinity/manager.go 证明删除成员同时维护 group region_count
和当前版本 affinity_region_count；placement/rule_manager.go 删除实际 region cache。

生产路径：[overlaps.rs](../../../pkg/cluster/overlaps.rs)::handle_overlaps，
类型化取消为 OverlapsCancelled { processed }，保持已完成前缀。
[领域说明](../../../pkg/cluster/overlaps.rs.md) 说明真实所有权与逐区域观察边界。
共享 clear_overlap_statistics 同时用于主题5 region_overlap 的 opt-in 组合，
没有第二份异常统计/label 算法。原 HandleOverlaps effect 改为调用此入口，
HeartbeatEffectsState 新增可选 affinity manager，状态锁下操作同一份缓存。
StatePipeline 的既有 submit_region_heartbeat 入口自然接入，未新增 heartbeat 路由。
空列表、可选 stats、可选 affinity、重复 ID 均保留 Go 语义；热点历史继续由原 TTL 管理。

[fixture](fixtures/task_128.json) 的前5行在实现前从 Go 源码冻结，后补2行可选/取消
组合及显式 inputs/expected_outputs。所有期望按来源推导，不从待测 Rust 输出生成，
无字段归一化。tests 用 fixture 的 flags/processed 及来源不变量断言真实状态。

| 测试 | 有效案例与观察 |
| --- | --- |
| task_128_behavior_cases | 7行：全安装、无stats、无affinity、开始前取消、完成1个后取消、两者都缺省、完成2个后取消；每行检查实际分类成员、placement cache、affinity成员/当前计数、延迟label计数，并重复清理及检查取消空列表 |
| task_128_pipeline_overlap_cleanup_and_stopped_lane | 2序列：现有真实heartbeat合并淘汰两个区域并清四类状态；misc runner停止时报告HandleOverlaps Admission且保持已接受root与未清缓存。正常序列还验证重复报告不使计数下溢 |

合计2项真实行为测试、9个顶层场景（另有每行的重复清理/空列表断言）。
使用真实 BasicCluster、RegionStatistics、LabelStatistics、RuleManager、Manager、
RegionLabeler、MemoryStore/StorageEndpoint 及 ConcurrentRunner；仅控制外部取消
检查与runner生命周期，不mock算法。清理无MetaStore I/O，不虚构存储故障。

首轮 [red](logs/task_128/red.json) 为 --locked 拒绝缺失本地路径依赖的接线错误，
不作行为红灯。Cargo.lock 最终仅增加6条已有workspace路径依赖，远程依赖无变化。
[behavior-red](logs/task_128/behavior-red.json) 对“返回成功但不清缓存”的临时错误实现
实际运行1项，0通过/1失败，exit101，断言 evicted stats 仍 observed。
[green](logs/task_128/green.json) 恢复正确实现后聚焦1项/包级6项通过。
[integrated](logs/task_128/integrated.json) 加入真实heartbeat验证后两个聚焦各1项通过。

[mutations.py](logs/task_128/mutations.py) 逐次临时跳过统计、label、affinity、placement
或取消检查，并在 finally 恢复生产源。stats-red、labels-red、affinity-red、
placement-red、cancel-red 的 JSON 与压缩日志分别记录同一行为测试0通过/1失败、
exit101；取消错误得到 Ok(3) 而期望 Err(OverlapsCancelled { processed: 0 })。
这些错误实现均已恢复，没有提交生产注入点或忽略测试。

首轮workspace测试编译在兼容性回归宿主缺少新 affinity 字段时报E0063，
exit101，记录于 [baseline-first.json](logs/task_128/baseline-first.json)。随后
核对全仓所有 HeartbeatEffectsState 初始化，除本包两个测试宿主外，为
`tests/rust-compat/state_regression.rs` 与 `pkg/server/sync_transport_test.rs`
各增加一行 `affinity: None`；这是新字段的必要测试接线，没有迁移相邻业务。

最终命令、退出码、有效数量和槽位记录见 [final.json](logs/task_128/final.json)。
[verify.py](logs/task_128/verify.py) 每阶段以原子 mkdir 动态领取仓库槽位，
owner 记录计划、任务、会话与持有PID；所有Cargo/子进程继承绝对 CARGO_TARGET_DIR、
GOFLAGS=-mod=readonly。正常、失败及中断路径先等候/终止子进程，然后删除自有锁，
保留编译缓存。未新增 feature。

| 最终命令 | 退出码 | 有效测试结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 检查通过 |
| `cargo test -p astersql-pd-cluster-utils --locked --lib task_128 -- --nocapture` | 0 | 1通过、0失败、0忽略 |
| `cargo test -p astersql-pd-cluster-utils --locked` | 0 | 6通过、0失败、0忽略 |
| `cargo test -p astersql-pd-cluster --locked --lib task_128 -- --nocapture` | 0 | 1通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查通过 |
| `cargo fmt --all -- --check` | 0 | 检查通过 |
| `cargo check --workspace --locked` | 0 | 检查通过 |
| `cargo test --workspace --locked` | 0 | 1825通过、0失败、1忽略 |

[汇总](logs/task_128/test-summary.json) 来自完整 stdout 的 test result；1825项包含
两项既有 compile-fail 文档测试，1项忽略为既有测试，本任务没有新增忽略。
仅有既有 proto deprecated 字段 warning。
所有阶段实际动态领取槽位1，绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
全部使用槽位产物的子进程均已退出，自有锁已释放，缓存保留。

[post-format](logs/task_128/post-format.json) 另行执行 cargo fmt --all 与格式检查均exit0；
[final-rust-hashes](logs/task_128/final-rust-hashes.json) 与再次格式化后逐文件核对完全一致，
最终测试验证的Rust内容没有发生变化。
[只读复核](logs/task_128/verification.json) 记录1101份Go/模块文件（含未跟踪路径）和
两份只读plan.md的内容与路径完全不变，Go/模块Git状态为空。
`git diff --check`、暂存差异检查与本任务文档相对链接检查通过。
提交排除其它任务已有的删除；编号任务完成后按要求删除，不单独提交该删除。
状态：已完成。

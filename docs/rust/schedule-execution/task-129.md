# 任务129：最新区域统计收集桥接

依赖128已核实提交 `a326ca19a` 及 [task-128.md](task-128.md)：真实 overlap
清理桥接和 heartbeat 接线已完成，workspace 1825项通过。整体和包级 plan.md 只读。

本任务采用用户后续明确授权的 Go 只读源码推导 + Rust 真实行为/故障验证。
`oracle-index.json` 没有本行为冻结运行输出；`pkg/cluster/cluster_test.go` 仅有热点
benchmark，不能冒充 Collect 的运行证据。本任务没有运行 Go 或生成/修改 Go 文件，
没有 Go binary/output 哈希，也不声称 Go/Rust 运行差分。
[provenance](logs/task_129/provenance.json) 固定来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、5份只读源码哈希与 fixture 哈希。

`pkg/cluster/cluster.go::Collect` 先取 BasicCluster，再以事件 ID 重读 root，缺失时
返回；之后检查 context，最后按当前 region 获取 peer stores 并 Observe。
`region_collection.go::Observe` 以当前 replicas/size/keys 计算分类，registered removing
peer store 产生 OfflinePeer；placement 未初始化时不修改统计。
`basic_cluster.go::GetRegionStores` 只取 region 的 store IDs，跳过未注册节点。
所有期望均从这些源码推导，在实现前冻结到 [fixture](fixtures/task_129.json)，
无字段归一化。旧事件 epoch1/单peer/size0，新 root epoch2/3peers/size40/400000keys，
期望健康bitmap0；store3 offline 为16；store4 offline 不属于当前peer不能污染结果。

生产入口 `pkg/cluster/collect.rs::collect` 和 `CollectCancelled` 复用真实 core、
statistics、config、placement 契约；[领域说明](../../../pkg/cluster/collect.rs.md)。
现有 `server/cluster/heartbeat_effects.rs::submit_collect` 直接调用桥接，在同一状态锁
内读取最新 root 和统计；StatePipeline 无需新入口。仅将已有 config 从 dev-dependency
提升为普通依赖；Cargo.lock 无需改变，没有 feature 变化。

`task_129_behavior_cases` 为1项真实测试、7个 fixture 场景：最新epoch、无BasicCluster、
区域删除、取消、最新peer store offline、缺失peer store、未初始化placement。
另外对可达且placement关闭的4行检查取消保留已观察统计，并在恢复后替换旧分类。
使用真实 BasicCluster/RegionStatistics/RuleManager/MemoryStore，取消为外部边界控制。
`task_129_queued_collection_latest_or_deleted_root` 为1项真实heartbeat测试、2序列：
用真实 ConcurrencyLimiter 延迟 misc 队列；执行前直接更新 root 到epoch2、size144并将
store3置为offline，断言 OversizedRegion/OfflinePeer 且不再 UndersizedRegion；
另一个序列删除root，断言统计未观察该ID。所有runner均join。

[red](logs/task_129/red.json) 初轮为缺失collect模块的编译红灯，不算行为失败。
[mutations.py](logs/task_129/mutations.py) 每次临时错误实现后运行同一聚焦测试并在finally
恢复：stale-red 使用旧事件产生 MissPeer；cancel-red 忽略取消仍更新真实统计；
stores-red 按旧事件peer查store，遗漏当前store3的 OfflinePeer。
对应JSON和压缩原始stdout/stderr保存退出码101、实际1项失败，不能只靠缺API验收。
最终恢复正确源码后运行验证，结果见 final.json。

最终验证（每条退出码均为0，原始输出与命令见 [final.json](logs/task_129/final.json)）：

| 命令 | 有效测试 |
| --- | --- |
| `cargo fmt --all` | 检查通过 |
| `cargo test -p astersql-pd-cluster-utils --locked --lib task_129 -- --nocapture` | 1通过/0失败/0忽略 |
| `cargo test -p astersql-pd-cluster-utils --locked` | 7通过/0失败/0忽略 |
| `cargo test -p astersql-pd-cluster --locked --lib task_129 -- --nocapture` | 1通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 检查通过 |
| `cargo fmt --all -- --check` | 检查通过 |
| `cargo check --workspace --locked` | 检查通过 |
| `cargo test --workspace --locked` | 5通过/0失败/0忽略, 12通过/0失败/0忽略, 17通过/0失败/0忽略, 49通过/0失败/0忽略, 7通过/0失败/0忽略, 1通过/0失败/0忽略, 7通过/0失败/0忽略, 10通过/0失败/0忽略, 10通过/0失败/0忽略, 15通过/0失败/0忽略, 160通过/0失败/0忽略, 4通过/0失败/0忽略, 19通过/0失败/0忽略, 2通过/0失败/0忽略, 7通过/0失败/0忽略, 12通过/0失败/0忽略, 1通过/0失败/0忽略, 9通过/0失败/0忽略, 5通过/0失败/0忽略, 5通过/0失败/0忽略, 2通过/0失败/0忽略, 13通过/0失败/0忽略, 2通过/0失败/0忽略, 10通过/0失败/0忽略, 2通过/0失败/0忽略, 3通过/0失败/0忽略, 9通过/0失败/0忽略, 5通过/0失败/0忽略, 1通过/0失败/0忽略, 1通过/0失败/0忽略, 17通过/0失败/0忽略, 8通过/0失败/0忽略, 7通过/0失败/0忽略, 29通过/0失败/0忽略, 3通过/0失败/0忽略, 19通过/0失败/0忽略, 2通过/0失败/0忽略, 3通过/0失败/0忽略, 28通过/0失败/0忽略, 44通过/0失败/0忽略, 66通过/0失败/0忽略, 20通过/0失败/0忽略, 14通过/0失败/0忽略, 3通过/0失败/0忽略, 27通过/0失败/0忽略, 45通过/0失败/0忽略, 10通过/0失败/0忽略, 12通过/0失败/0忽略, 37通过/0失败/0忽略, 95通过/0失败/0忽略, 44通过/0失败/0忽略, 19通过/0失败/0忽略, 10通过/0失败/0忽略, 12通过/0失败/0忽略, 192通过/0失败/0忽略, 13通过/0失败/0忽略, 5通过/0失败/0忽略, 48通过/0失败/0忽略, 1通过/0失败/0忽略, 93通过/0失败/0忽略, 4通过/0失败/0忽略, 124通过/0失败/0忽略, 31通过/0失败/0忽略, 16通过/0失败/0忽略, 17通过/0失败/0忽略, 10通过/0失败/0忽略, 103通过/0失败/0忽略, 30通过/0失败/0忽略, 10通过/0失败/0忽略, 21通过/0失败/0忽略, 85通过/0失败/0忽略, 9通过/0失败/0忽略, 2通过/0失败/0忽略, 3通过/0失败/0忽略, 9通过/0失败/0忽略, 10通过/0失败/0忽略, 10通过/0失败/0忽略, 2通过/0失败/0忽略 |

workspace合计 1827通过、0失败、1既有忽略，包含2项既有compile-fail文档测试；
本任务新增2项真实测试/9个顶层场景，未新增忽略。只有既有proto deprecated字段警告。
所有阶段实际动态领取槽位1，绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[verify.py](logs/task_129/verify.py) 使用原子mkdir领取槽位，记录plan/task/session/PID，
所有Cargo及其子进程继承绝对路径与GOFLAGS=-mod=readonly。正常、失败和中断均先等待
或终止子进程，再清理自有owner与锁；本任务进程均已结束、锁已释放，保留缓存。

[只读复核](logs/task_129/verification.json) 核对1101份Go/模块文件和两份plan.md
路径与内容不变，Go/模块Git状态为空；git diff --check与相对链接检查通过。
最终Rust文件哈希见 [final-rust-hashes](logs/task_129/final-rust-hashes.json)。
提交仅包含本任务实现、测试、局部接线与证据，排除其它任务删除；编号任务文件完成后删除。
状态：已完成。

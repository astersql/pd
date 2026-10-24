# 任务22：热点过期与冷副本清理

依赖task-21.md及累计1–20完成证据已核对。开始HEAD
`6b87115acdedfc3c120c20d8f67ad8d17d200ba8`；Go基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，参考Go生产/测试无基准差异。
已读两份只读计划、根AGENTS、Copilot、ADR、依赖与移植指南，无更深AGENTS。

## 实现前oracle

只读审查hot_peer_cache.go的冷报告、put/remove/gc、继承及TopN TTL链路，
原测试TestRemoveExpireItems/TestRemoveFromCache/TestRemoveFromCacheRandom、
buildRegion/checkFlow/updateFlow、TestMain/goleak及testutil等待函数。
选定路径不需failpoint、不写Go/模块、不调用生成/格式化/整理。
运行 `GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^(TestRemoveExpireItems|TestRemoveFromCache|TestRemoveFromCacheRandom)$' -count=1 -v`。
退出0，3个顶层测试通过，[实际输出](task22/go-oracle.out)。
原测试TTL=100ms，region1三副本插入、等待、region2报告触发GC，region1索引空、region2非空；
再等待、ResetStores后region3报告，旧store缓存不存在、新store反向索引非空。
write三副本5秒报告暖机200次、错开窗口、零负载至清空；随机移动测试覆盖3/5副本、
120/60/10/5秒、两种报告顺序，最终冷缓存清空。Rust使用固定移动序列。
Go输出为原断言运行证据，不是逐字段JSON快照。

补充源码契约：缺失报告产生零瞬时loads的inCold动作；真实滚动窗口累计至报告边界
强制degree/anti各减1，anti<=0移除，部分窗口保留。Go冷路径将阈值写入delta前四项
再按方向取值（保留该现有布局，CPU为0），不将其解释为新报告流量。
TTL默认3倍方向报告间隔，每次应用Add/Update刷新；GC间隔TTL，等于边界可运行，
过期deadline<=now即清理；store输入由宿主排除removed状态。缺失region靠TTL或显式remove_region。
同步领域内存策略，不引入MetaStore、后台任务或调度依赖。

## 实现与失败证据

先注册最小可编译API及独立hot_expire_test，再运行
`bash docs/rust/state/task22/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_22_hot_expire -- --nocapture'`。
[红灯](task22/red.out)退出101，0 passed、6 failed、0 ignored：冷报告返回0而预期1、
TTL后peer仍存在、remove_region未删除、旧store阈值仍80000而预期最小值等真实断言。
首次格式化发现错误的字段可见性文本替换，已修复；该解析错误不计行为红灯。
[首轮绿灯](task22/green.out)同命令退出0，6 passed、0 failed/ignored。
最终补充GC移除store1而保留尚未过期store2及其TopN阈值断言。

[实现](../../../pkg/statistics/hot_expire.rs)公开HotPeerCache::check_cold_peer、gc、
set_peer_ttl、remove_region、stores_of_region与regions_of_store；
[应用动作](../../../pkg/statistics/hot_peer_update.rs)增加update_stat_at并复用冷热状态机，
[阈值](../../../pkg/statistics/hot_threshold.rs)增加按存活store集合清理，包含threshold-only store。
[独立测试](../../../pkg/statistics/hot_expire_test.rs)显式登记，未新增依赖或Cargo.lock变化。

事件断言包括：read缺失region200、已报告region100和其他store隔离；4+6秒窗口在10秒边界
降degree/anti，11次后移除；零瞬时loads和丢弃动作不改窗口；read/write anti耗尽移除；
TTL 100ns、store2在50ns刷新、99ns不GC、100ns只清store1、199ns节流、200ns清store2；
方向默认TTL=3*reportInterval；时钟回退；store移除时同时清peer和阈值，store2保留60个region；
region显式消失幂等清全部副本，TopN从60降59后阈值回最小；3/5副本及120/60/10/5秒的
固定移动序列后零负载最终全部清空。

双向成员查询来自canonical peers索引，不复制Go空map/tombstone。Go在任一store的peer过期时
删除整个region的辅助索引，可能隐藏其他store尚有效peer；Rust对每个过期peer精确清理并保持
尚有效成员可查。这一非均匀TTL场景在Rust单独验证，已在相邻rs.md声明内部表示差异；
Go原测试覆盖同时过期和冷副本最终清空。缺失报告inCold的布局/窗口分支为只读源码契约，
不声称上述既有Go测试直接调用该未单列测试的入口，也不声称生成了逐字段JSON运行差分。

本任务没有持久化、网络或后台任务，不需要新增MemoryStore/EtcdMetaStore或cancel/join接口。
workspace既有真实Memory/Etcd回归沿用已审查的etcd_fixture_test.rs、rust-etcd-fixture/main.go及
proto/build.rs；Go构建只读现有源码，proto校验固定ZIP后仅在OUT_DIR生成Rust。
槽位包装器对子进程继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off；
Etcd夹具EOF退出，必要时kill并wait；没有make/failpoint/Go格式化或模块整理。

## 最终验证与退出证据

入口 `bash docs/rust/state/task22/slot.sh bash docs/rust/state/task22/validate.sh`，
[验证脚本](task22/validate.sh)、[完整日志](task22/validation.out)、[metadata](task22/metadata.out)、
[逐命令统计](task22/summary.json)。红灯、首轮绿灯、最终回归均由[槽位包装器](task22/slot.sh)
原子mkdir动态领取槽位1，记录计划/任务/会话/PID；正常、失败、中断先等待或终止并wait子进程再释放。
实际 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，缓存保留。

| 命令 | 退出码 / 有效测试数 |
| --- | --- |
| 上述只读Go oracle | 0；3个顶层测试通过 |
| `cargo fmt --all` | 0；无范围外格式化改动 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_22_hot_expire -- --nocapture` | 0；6 passed、0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0；74 passed、0 failed/ignored |
| `cargo check --workspace --locked` | 0；只有既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0；1089 passed、0 failed、1既有ignored |
| `python3 docs/rust/state/task22/readonly.py` | 0；内容/状态/计划/链接/diff检查通过 |

workspace包含真实MemoryStore/EtcdMetaStore契约；既有ignored为tso_processes，不计本行为验收。
[退出检查](task22/resources.out)确认自有slot-1锁不存在、无本槽位产物进程，缓存保留。
[只读检查](task22/readonly.out)确认1101个Go/模块文件集合、SHA256和Git状态与起点相同；
[起始哈希](task22/go-before.json)、[起始状态](task22/go-status-before.out)，aggregate
SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
两份只读plan.md与[起始哈希](task22/readonly-plans.json)一致。用户已有计划变更保留且不提交。

状态：已完成。仅验收热点过期与冷副本清理，不表示整个统计主题完成。

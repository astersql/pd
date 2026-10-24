# 任务23：热点任务执行与关闭

开始HEAD `7db0fea9daebf94b640212b206dd43f2be5b4757`；依赖task-22累计1–22证据齐全。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，hot_cache.go/test.go无后续差异。
已读取两份只读计划、根AGENTS、Copilot、ADR、依赖基线和移植指南，无更深AGENTS。

## 实现前oracle

只读审查hot_cache.go/test.go、HotPeerCache查询/flow/expired路径、buildRegion、
newBenchmarkRegion、原闭包newOriginalRegionFlowTasks、TestMain/goleak等待链路。
没有所选路径的failpoint或Go/模块写入，也不运行make、注入、生成、格式化或依赖整理。
已运行 `GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^(TestRegionFlowTasksMatchOriginalForAnyPeerCount|TestIsHot)$' -count=1 -v`。
退出0，2个顶层测试及6个peer子测试通过，[实际日志](task23/go-oracle.out)。
0/1/2/3/5/9副本：region1先有n+1副本，然后缩为n；每次write字节1048576、keys1024、
query0、60秒，leader为第一个peer/空。原闭包和compact任务运行后完整HotPeerStats及双向索引相等，
多余store=n+1不存在。IsHot测试在read/write方向分别Update一个peer后各重复查询100次为true。
Go断言运行证据未输出JSON，不冒充逐字段新捕获。

源码契约：任务仅持有meta、leader store、wrapping秒差、bytes/keys/query；write其余维度为0。
每个方向FIFO，expired先于write；读报告显式单store、冷报告按store已报告ID集合。
GetHotPeerStats要求degree>=min、非inCold且anti等于方向默认值；IsHot仅检查当前region成员的degree。
接受入队仅是接受，取消与拒绝不能宣称Applied。Rust采用宿主Tokio handle，显式cancel/join和有界队列，
以错误区分Full/Closed/Canceled/WorkerStopped。Go无join/可靠取消结果，此部分为Rust生命周期增强。

Go/模块起点SHA256/集合与状态已保存于task23；两份只读plan哈希已保存。

## 实现、红绿证据与边界

先登记最小可编译run_task接口（未应用动作）及独立测试，执行
`bash docs/rust/state/task23/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_23_hot_worker -- --nocapture'`。
[红灯](task23/red.out)退出101，2 passed、2 failed、0 ignored：完整write状态为空而预期有peer，
read热点查询返回false。不是编译报错或零测试；capacity/cancel及字段捕获已有实现通过。
同命令[首轮绿灯](task23/green.out)退出0，4 passed；随后扩充为
[8个行为测试](task23/expanded.out)，退出0、无失败/忽略。

[实现](../../../pkg/statistics/hot_worker.rs)增加HotCache、HotTask、HotTaskReceipt、
HotTaskOutput、HotTaskError、new_region_flow_tasks和HotPeerCache::run_task。
[模块登记](../../../pkg/statistics/lib.rs)、[行为说明](../../../pkg/statistics/hot_worker.rs.md)、
[独立测试](../../../pkg/statistics/hot_worker_test.rs)均在canonical统计crate。
将既有tokio workspace依赖从dev移至production；不新增依赖版本或Cargo.lock变化。
补充[core report_interval](../../../pkg/core/region_query.rs)只读getter，避免外部猜测/重构报告间隔；
[说明](../../../pkg/core/region_query.rs.md)与测试固定wrapping 60秒、write bytes/keys/query捕获。

Rust执行真实HotPeerCache/DimStat/TopN，不mock统计结果。同步原路径与异步队列对比完整peer map
的Debug字段（含瞬时loads、滚动窗口、last average、degree、anti、stores、leader、action、
继承及迁移时间），并验证过期store消失。另以48条queued read/冷/过期事件检查FIFO、完整窗口、
TopN阈值和后续TTL清空一致。Go原测试运行输出提供既有断言证据，Rust字段/窗口对照是Rust
同步原路径与异步路径的差分，不声称Go输出了新JSON。

查询返回owned去噪快照，调用方修改不影响缓存；read/write各100次IsHot、min degree、
空membership、方向隔离及Stats筛选均验证。capacity=1确定性制造Full和heartbeat部分拒绝；
receipt取消成功后断言Canceled且无缓存变更；关闭后Closed，待执行任务Canceled，shutdown
join两worker返回空缓存，重复关闭None。共享两线程host上128次cancel/claim竞争校验真实Applied
数量与最终hot_degree对应；runtime销毁返回WorkerStopped，Drop取消不误报应用。
关闭后宿主仍能spawn任务。shutdown须完整await，Drop不提供join保证，此限制已公开说明。

本任务仅内存热点编排，无MetaStore持久化或网络任务，因此不新增后端接线/伪造存储场景。
workspace回归使用既有真实MemoryStore/EtcdMetaStore契约。已审查etcd_fixture_test.rs、
scripts/rust-etcd-fixture/main.go和proto/build.rs：现有Go夹具只读构建至槽位，EOF请求退出，
必要时kill/wait；proto固定ZIP校验/既有缓存，不修改Go源码或模块。命令统一继承
GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。


首次workspace回归在既有syncutil::singleflight_randomized失败（首次poll必须Pending断言），
见[首次完整日志](task23/validation.out)。本任务未改syncutil；该多线程随机测试允许独立运行
已在首poll时完成的任务，其注册助手却强制Pending。只读审查后复跑
`cargo test -p astersql-pd-syncutil --locked --lib singleflight_randomized -- --nocapture`，
退出0、1 passed；保留原失败，不修改无关测试或隐藏失败。
随后同槽位运行全量workspace复验，[复验日志](task23/workspace-retry.out)。

## 最终验证、资源与只读证据

[验证入口](task23/validate.sh)由[槽位包装器](task23/slot.sh)运行；
所有Cargo阶段原子mkdir动态领取槽位1，写入计划/任务/会话/PID，子进程继承
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[完整初验日志](task23/validation.out)、[复验日志](task23/workspace-retry.out)、
[逐命令统计](task23/summary.json)、[metadata](task23/metadata.out)。

| 命令 | 退出码 / 有效测试数 |
| --- | --- |
| 上述只读Go oracle | 0；2个顶层测试、6个peer子测试 |
| `cargo fmt --all` | 0；无范围外格式化改动 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_23_hot_worker -- --nocapture` | 0；8 passed、0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0；82 passed、0 failed/ignored |
| `cargo check --workspace --locked` | 0；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` 首次 | 101；1060 passed、1 failed、1既有ignored；上述syncutil竞争 |
| `cargo test -p astersql-pd-syncutil --locked --lib singleflight_randomized -- --nocapture` | 0；1 passed |
| `cargo test --workspace --locked` 复验 | 0；1097 passed、0 failed、1既有ignored |
| `python3 docs/rust/state/task23/readonly.py` | 0；Go内容/状态、计划哈希、链接、diff检查 |

workspace真实MemoryStore/EtcdMetaStore契约通过，既有ignored是tso_processes，不计本行为验收。
[退出检查](task23/resources.out)确认自有锁不存在、没有本槽位产物进程，缓存保留。
[只读核对](task23/readonly.out)确认1101个Go/模块文件集合、SHA256、Git状态不变，
[起始哈希](task23/go-before.json)、[起始状态](task23/go-status-before.out)，aggregate
SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
两份只读plan.md与[起始哈希](task23/readonly-plans.json)一致。用户已有计划变更不纳入提交。

状态：已完成。仅验收热点任务执行与关闭，不表示整个统计主题完成。

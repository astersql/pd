# 任务15：区域异常分类索引

实现前 HEAD `d9efd92dfc3cdcddea33354871c6fafa33fce63e`；Go 基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，region_collection.go/test.go 无后续差异。
依赖 [task-14.md](task-14.md) 已验收累计1–14。已读两份只读plan、根规则、Copilot、ADR与移植指南；无更深AGENTS。

## 实现前 oracle

`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^TestRegionStatistics$' -count=1 -v`
退出0，1个测试通过：[输出](task15/go-oracle.out)。已审查 TestMain/goleak/WaitForEtcdConnections、NewTestOptions、
内存Storage、RuleManager.Initialize及Observe调用链；该路径不需failpoint，不执行生成器或改写Go/模块。
只使用已有测试。原测试的require断言提供运行状态证据，无逐事件JSON输出。

事件：4peer含store7 learner/offline，初始extra/learner/empty/undersized/offline均1；
size1 keys2清除empty；再加2down/1pending及size144，extra/down/pending/learner/oversized/offline均1，miss/undersized为0；
加入2peer的region2含1down后miss1/down2/undersized1；删除region1的store7后extra/learner/offline归零；store7恢复不重新产生offline。
单位：size MiB，keys计数；bit按miss到witness依次1<<0..9。
源码推导（区别于运行差分）：oversized为size或keys >=，merge为两者 <=，empty需size<=1且keys=0；
NeedUpdate只看首次/oversized/pending/down/learner/empty/undersized，不检查peer数量/offline/witness。
持续时间为Unix秒，从首次观察到持续异常开始，重复观察输出累计值，清除后重新计时；Go使用0时间戳哨兵。
查询使用当前provider并clone，缺失region跳过；ClearDefunctRegion清除所有索引，包括零bit已观察记录。
本任务仅placement禁用分支；启用返回显式错误，留待后续放置集成，不能静默使用副本数替代规则。

补充只读运行 `GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^TestRegionStatisticsWithPlacementRule$' -count=1 -v`，
退出0，1个测试通过，见 [witness输出](task15/go-witness-oracle.out)。审查同一内存RuleManager调用链，不需failpoint。
其region5的witness leader断言通过；只借此确认witness观测，未宣称Rust支持placement规则分支。
Rust等价重放第一个测试，将peer/store ID做一对一重命名（store7→4），保留所有事件及分类快照。

## 测试驱动与实现

最小可编译observe接口先返回空结果：
`bash docs/rust/state/task15/slot.sh bash -c 'cargo fmt --all && cargo check -p astersql-pd-statistics --offline && cargo test -p astersql-pd-statistics --locked --lib state_task_15_region_classify -- --nocapture'`。
退出101，5 failed/0 passed，见 [红灯日志](task15/red.out)：bitmap None应为Some(370)/Some(512)/Some(0)，
duration (None,None)应为(Some(7),Some(7))，已观察后NeedUpdate仍为true。
离线check先维护新增现有canonical schedule-config依赖的Cargo.lock边，不改变版本；后续全部验证使用--locked。

实现 [region_classify.rs](../../../pkg/statistics/region_classify.rs) 的RegionStatistics双向索引，
observe/is_region_stats_type/get_region_stats_by_type/clear_defunct_region/region_stats_need_update，
以及显式时钟duration样本。所有分类使用真实core与ProviderSnapshot；没有模拟分类结果。
[行为说明](../../../pkg/statistics/region_classify.rs.md)记录阈值、查询owned快照、零bit观察状态及边界。
仅新增现有schedule-config依赖和lib显式模块登记，未创建平行配置或placement实现。

## 资源与只读边界

[槽位包装器](task15/slot.sh)每阶段原子mkdir动态领取1–10，记录计划/任务/会话/PID；
失败、成功及信号退出均等待子进程后释放自有锁，缓存保留。Cargo及子进程继承绝对CARGO_TARGET_DIR、
GOARCH=arm64/CGO_ENABLED=0/GOFLAGS=-mod=readonly/GOPROXY=off。
已审查pkg/proto/build.rs，仅固定ZIP校验与槽位OUT_DIR生成；
pkg/storage/kv/etcd_fixture_test.rs及scripts/rust-etcd-fixture调用链仅只读go build，
产物/数据留在槽位；Drop关闭stdin，必要时kill并wait，不执行Go格式化/注入/生成器。
本任务为同步纯内存索引，无后台任务或后端访问；不为此另造MemoryStore/EtcdMetaStore测试。
全workspace回归仍覆盖真实MemoryStore及EtcdMetaStore，并验证其退出。

## 最终验证

[验证脚本](task15/validate.sh)、[完整输出](task15/validation.out)与[metadata](task15/metadata.out)
记录最终版本的确切命令。初次全部通过后补充多分类清除、nil leader、keys配置重判断言，再重复最终验证。

| 命令 | 退出码/有效测试数 |
| --- | --- |
| 两条上述只读Go oracle命令 | 各0，各1 passed |
| `cargo fmt --all` | 0，无范围外格式化变化 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_15_region_classify -- --nocapture` | 0，5 passed/0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0，37 passed/0 failed/ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，1052 passed/0 failed，1既有ignored |
| `python3 docs/rust/state/task15/readonly.py` | 0，1101个Go/模块文件集合、SHA256及Git状态不变，链接与diff检查通过 |

workspace ignored为既有tso_processes，不计本任务验收。真实MemoryStore/EtcdMetaStore回归通过。
所有Cargo阶段实际槽位1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[资源核对](task15/resources.out)确认自有锁/owner释放、槽位产物进程全部退出、缓存保留。
[只读核对](task15/readonly.out)、[起点哈希](task15/go-before.json)及[起点状态](task15/go-status-before.out)
证明无新增/变化Go或模块文件；aggregate SHA256
`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
两份指定plan.md从未修改；保留其他任务原有变更。

状态：已完成；只验收placement禁用的异常分类索引边界。

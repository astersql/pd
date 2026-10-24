# 任务24：分引擎负载聚合

开始HEAD `f7b890ba182f30d43f8c8055b1fd65781851e50d`；task-23累计依赖已完成。
已阅读两份只读plan、AGENTS、Copilot、ADR、依赖基线、移植指南，无更深AGENTS。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；本任务四个Go参考文件无后续差异。

实现前只读审查collector.go、store_hot_peers_infos.go、store_load.go、
store_collection_test.go的四个指定测试、HotPeerStat.GetLoad/Clone、core引擎/transfer过滤、
StoreHistoryLoads采样、TestMain与WaitForEtcdConnections/goleak调用链。
所选路径无failpoint依赖，不写Go或模块；不运行make、注入、生成或整理。
命令 `GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^(TestSummaryStoreInfos|TestSummaryStoresLoadByEngineZeroCPUStddev|TestTiFlashComputeExcludedFromExpectation|TestTiKVNotAffectedByTiFlashCompute)$' -count=1 -v`
退出0，4 passed；[实际运行输出](task24/go-oracle.out)。这是原测试断言运行证据，不声称输出了新的字段JSON。

契约与事件：TiKV ID1/3读leader输入StoreKindLoads{1,2,0,0,5}乘ID；首次current历史为空，
先读历史后采样，后续history按物理槽保存[1,2,5,0]乘ID，期望[2,4,10,0]。
零CPU测试ID1/2读[100,10,5,0]乘ID、各1个leader peer，CPU归一化stddev必须0且uniform。
TiFlash write ID1–3各5 MiB/s、1000 keys/s，compute ID4/5为0；期望5 MiB/s且仅3节点。
TiKV ID1–3各10 MiB/s、2000 keys/s，compute ID4/5为0；期望10 MiB/s，TiFlash集合空。
源码补充契约：TiKV leader需transfer双向允许，region不看该开关；TiFlash仅write节点region。
读TiKV取store四维，写leader字节/keys取去噪peer和、query取store，写region取store字节/keys。
TiFlash读为0，写region按trace开关取regions store计数或peer和。缺失storeLoads跳过。
每引擎独立均值；历史缺失槽按0计入全人口平均；无热点时stddev全0，非零热点时
sqrt(variance/N)/mean，零均值维置0；stddev peer count沿用期望count。pending只影响future。
本任务为同步内存聚合，无后台任务、存储I/O或新增协议/运行时。

## 实现与红绿证据

新增[engine_load.rs](../../../pkg/statistics/engine_load.rs)、
[独立测试](../../../pkg/statistics/engine_load_test.rs)和
[行为说明](../../../pkg/statistics/engine_load.rs.md)，在既有canonical crate显式登记。
导出summary_stores_load、StoreLoadDetail、StoreLoadSummary；复用既有summary_store_infos，
不重复建模，无新增依赖、feature、Cargo.lock改动。collector内部按引擎选择规则，
返回按ID排序的owned快照；宿主提供时钟，生命周期与存储边界不变。指标留后续任务。

先注册可编译空集合接口和四个行为测试，执行
`bash docs/rust/state/task24/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_24_engine_load -- --nocapture'`。
[红灯](task24/red.out)退出101、0 passed/4 failed，具体断言为人口0而期望2、
ID集合[]而期望[1,2,3]；不是编译错误/零测试。实现后同命令
[首轮绿灯](task24/green.out)退出0、4 passed。
[扩充验证](task24/expanded.out)退出0、7 passed、无失败/忽略。
最终迭代从metadata值读取store ID时修复两处借用错误，保留
[该次编译日志](task24/refactor-compile.out)，不作为行为红灯。

测试逐字段校验上述四项Go原测试行为，并扩展实际collector矩阵：TiKV读四维含CPU，
写leader仅leader peer和且query取store，写region只取store bytes/keys；
TiFlash trace取regions数据，关闭trace取peer和，读为零；compute、缺负载与未知kind排除。
热点用真实HotPeerStat::get_load/clone，取整11.4→11、12.6→13，leader筛选与owned隔离通过。
pending八维映射、peer count只影响future，expect不受pending与另一引擎污染。
双向transfer暂停只排除leader，removed仍能汇总region但不重新生成history。
增量summary覆盖空人口、不等长历史（缺槽0参与全人口平均）、非零热点归一化方差、
零CPU/零均值维、负均值符号、无热点时stddev为0和stddev count沿用期望。
这些补充断言来自已审查Go源码契约；不是另行捕获的Go JSON差分。

## 验证与只读/资源证据

[验证入口](task24/validate.sh)统一通过[槽位包装器](task24/slot.sh)运行。
每阶段动态原子mkdir领取槽位1，写入任务/计划/会话/PID；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
包装器signal/finally先终止并wait命令组，再释放自有锁，缓存保留。
已审查pkg/proto/build.rs固定模块ZIP下载/校验及OUT_DIR生成、
etcd_fixture_test.rs与既有scripts/rust-etcd-fixture/main.go：构建只写槽位产物，
数据只写槽位临时目录，不注入failpoint；EOF退出且Drop kill/wait兜底。
所有Cargo子进程继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。

[完整验证日志](task24/validation.out)、[逐命令统计](task24/summary.json)、
[metadata](task24/metadata.out)：

| 命令 | 退出码 / 有效测试数 |
| --- | --- |
| 上述只读Go oracle | 0；4 passed |
| `cargo fmt --all` | 0；无范围外格式化差异 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_24_engine_load -- --nocapture` | 0；7 passed、0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0；89 passed、0 failed/ignored |
| `cargo check --workspace --locked` | 0；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0；1104 passed、0 failed、1既有ignored |
| `python3 docs/rust/state/task24/readonly.py` | 0；Go/模块内容、状态、计划哈希、链接与diff检查 |

全workspace真实MemoryStore/EtcdMetaStore契约通过。既有ignored为tso_processes，
不计本任务行为验收。[资源退出证据](task24/resources.out)确认自有锁释放、槽位无进程，缓存保留。
[起始哈希](task24/go-before.json)、[起始状态](task24/go-status-before.out)、
[只读核对](task24/readonly.out)证明1101个Go/模块文件集合、SHA256、Git状态无差异；
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
[只读计划哈希](task24/readonly-plans.json)保持一致。未提交用户已有计划变更。

状态：已完成；仅验收分引擎负载聚合，不表示整个统计主题完成。

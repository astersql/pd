# 任务14：区域汇总与过滤

实现前HEAD `cd7389d263b1dea5f1b96372142d2e8cbfaf2eca`；Go基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，region.go及region_collection_test.go无后续差异。
已阅读两份只读plan、根规则、Copilot、ADR、依赖基线和移植指南，无更深AGENTS。
累计依赖验收见[task-13.md](task-13.md)。真实core已有RegionInfo统计与witness计算。
HotPeerStat窗口在后续批次18，本任务只定义已求值热点快照的领域输入，不创建缓存或窗口桩。

## 实现前oracle

`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^(TestRegionStatsObserveIAKVSize|TestRegionStatsEmptyCountUsesKeys)$' -count=1 -v`
退出0，2 passed，见[输出](task14/go-oracle.out)。IA 40/100保持40，120/100封顶100；
size=1且keys=0/1的两region仅1个empty；nil leader没有leader分布。
审查NewRegionInfo、SetApproximate*、GetRegionStats/Observe与TestMain/goleak包装：
这些纯内存测试不需要failpoint，不执行生成器或写Go/模块；readonly/off禁止模块更新。

扩展运行命令：`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test -tags without_dashboard ./tests/server/api -run '^TestStatTestSuite$/^TestRegionStats$' -count=1 -v`。
先审查statTestSuite、SchedulingTestEnvironment非微服务路径、NewTestCluster、Bootstrap、
MustPutRegionInfo、MustHandleStoreHeartbeat、TestMain和Cleanup：非微服务路径不Enable failpoint，
使用已有Go服务/内存统计/临时Etcd，关闭集群并取消context；源码及模块只读。
微服务路径的fastUpdateMember/highFrequencyClusterJobs不在选中路径；不运行make或注入。
退出0，1项行为子测试通过，见[运行输出](task14/go-api-oracle.out)。初次不带tag因dashboard依赖CGO未能编译，增加仓库已有without_dashboard tag后通过。

源码契约与运行证据分开：count/size/keys与IA封顶先于过滤；多个peer条件取AND；
nil leader仍有peer统计；witness的非零store size/keys为0；TiFlash learner与普通peer
都使用core的整region估计，不能将columnar值误当peer大小。StoreEngine由调用方填充，
Observe不修改。热点使用GetLoad已计算/舍入的byte/key/query每秒快照，不从heartbeat总量猜算。

API运行输入（stats_test.go，checkRegionStats）：四region依次为
`(id,peers,leader,size,kv,keys)` = `(1,[1,2,3],1,100,80,50)`、
`(2,[1,4,5],4,200,180,150)`、`(3,[1,5],5,1,1,0)`、`(4,[4],4,50,30,20)`。
每个region报告5次；region1写入100s内50000/5000/500每秒速率，随后store1报告
10s内10000/1000/100每秒读速率。全量快照(count,empty,size,kv,keys)为
`(4,1,351,291,220)`；子集region2/3为`(2,1,201,181,150)`。
leader size `{1:100,4:250,5:1}`、peer size `{1:301,2:100,3:100,4:250,5:201}`。
engine=tikv过滤只保留store1分布但全局总数与大小不变，热读`10000/1000/100`，
热写`50000/5000/500`。Rust使用同样的区域及已求值热点快照，精确比较各维map。
既有Go测试通过其全值断言证明这些状态，未输出逐快照JSON，不冒充新事件流捕获。
扩展的witness/TiFlash、溢出和多条件AND测试属于源码推导契约，并非新增Go运行差分。

## 测试驱动实施

显式登记region_totals与独立region_totals_test。最小可编译Observe空接口时执行
`bash docs/rust/state/task14/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_14_region_totals -- --nocapture'`，
[红灯日志](task14/red.out)：退出101，6个真实行为断言失败，0 passed。
具体包括IA得到0而预期40、全量得到全零而预期`(4,1,351,291,220)`、
witness/TiFlash peer count得到空map而预期`{1:1,2:1}`。
实现真实聚合后同一过滤器6项通过；无mock聚合/热度结果，仅热点输入边界存放值快照。

生产符号见[行为说明](../../../pkg/statistics/region_totals.rs.md)。只扩展canonical
statistics crate与lib登记，不改manifest/Cargo.lock、不引入protobuf、调度决策、
私有runtime、存储或全局状态。后续窗口将真实HotPeer.GetLoad值投影为快照接入，
本任务没有声称完成窗口/缓存生命周期。tests使用core真实RegionInfo与其peer大小计算。

## 资源与只读审查

[槽位包装器](task14/slot.sh)使用原子mkdir动态领取1–10，各阶段实际均为槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
记录计划/任务/会话/PID，正常/失败/信号退出等待子进程，释放自己的锁并保留缓存。
所有Cargo与子进程继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。

已审查pkg/proto/build.rs：固定ZIP校验摘要，仅向槽位OUT_DIR解压/生成；
既有Etcd夹具go build只读scripts/rust-etcd-fixture源码和Go模块，产物/数据在槽位内，
无failpoint/generate；Drop关闭stdin，等待退出，必要时kill再wait。
本任务聚合是同步值计算，没有后台任务需cancel/join，也不访问MemoryStore/EtcdMetaStore；
workspace回归仍验证既有真实两后端，不新增无关持久化测试。

## 最终验证

[验证脚本](task14/validate.sh)、[完整日志](task14/validation.out)与
[metadata](task14/metadata.out)保留确切命令。统一槽位包装运行，以下均退出0：

| 命令 | 有效测试数/结果 |
| --- | --- |
| 上述Go统计oracle命令 | 2 passed |
| 上述Go API oracle命令（without_dashboard） | 1项行为子测试passed |
| `cargo fmt --all` | 通过，无范围外Rust变更 |
| `cargo metadata --format-version 1 --no-deps` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_14_region_totals -- --nocapture` | 6 passed，0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 32 passed，0 failed/ignored |
| `cargo check --workspace --locked` | 通过，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 1047 passed，0 failed，1既有ignored |
| `python3 docs/rust/state/task14/readonly.py` | 1101个Go/模块文件集合、SHA256和Git状态不变，文档链接与diff检查通过 |

workspace既有ignored为tso_processes，不计本任务行为证据；真实MemoryStore和
EtcdMetaStore回归通过。[资源核对](task14/resources.out)确认槽位产物进程均退出，
自有锁和owner释放，缓存保留。实际槽位1，绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

[起点内容哈希](task14/go-before.json)、[起点Git状态](task14/go-status-before.out)和
[最终核对](task14/readonly.out)包含未跟踪Go文件；无新增或变化。
aggregate SHA256为`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
两份指定plan从未修改；其他计划的既有变更不属于本任务。

状态：已完成，无待回归项，仅验收区域汇总与过滤边界。

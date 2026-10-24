# 任务13：负载预测值代数

实现前 HEAD `a52343a899bb56f06b3fa5ddab4208c78df30ba8`；Go基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，store_load.go无基准差异。
已阅读两份只读plan、根规则、Copilot、ADR、依赖基线与移植指南，无更深规则。
批次12及累计依赖证据见[task-12.md](task-12.md)。复用Loads、HistoryLoads、
StoreKindLoads与statistics-utils的RWType/RegionStatKind，不新增依赖或调度实现。

## 实现前Go oracle

命令：`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/schedule/schedulers -run '^(TestInfluenceByRWType|TestHotReadRegionScheduleWithPendingInfluence|TestHotWriteRegionScheduleWithPendingInfluence)$' -count=1 -v`。
[运行输出](task13/go-oracle.out)：退出0，3 passed。
同命令增加 `-coverpkg=github.com/tikv/pd/pkg/statistics -coverprofile=docs/rust/state/task13/go-coverage.out`
再次退出0，3 passed，[运行输出](task13/go-oracle-coverage.out)、
[仅保留store_load.go的覆盖记录](task13/go-coverage.out)证明AddInfluence、ToLoadPred两方向、
Min/Max/Pending/Diff与MinLoad/MaxLoad的所有数值计算块均实际执行。
现有Go测试不输出逐个预测向量；不把间接调用覆盖冒充逐向量运行差分。

审查hot_region_test.go中的上述函数、prepareSchedulersTest、addRegionInfo、
summaryPendingInfluence、mockcluster.NewCluster、NewTestOptions、operator.NewController、
NewHotCache和TestMain：内存存储与内存报告，context cancel清理goroutine，goleak检查。
不运行generate/make、不写Go/模块；readonly/off禁止更新和下载。选中测试不启用或依赖
failpoint；persistFail、operator控制器故障、bucket/subtree注入并非这些断言的前提。

既有运行输入和断言：TestInfluenceByRWType按0.5MiB读写peer、0.7MiB write leader
累计source1负影响、dest4/dest3正影响，来源预期-0.5/+0.5与-1.2/+0.7 MiB/s。
注意其nearlyAbout使用OR，数值断言较弱，不能据此声称精确数值验证；Rust需精确断言。
读pending场景byte/key逐一执行，当前[7.1,6.1,6,5]MiB/s，移动0.5MiB/s两次，
源Min/Max为[6.6,7.1]再[6.1,7.1]，目标[5,5.5]再[5,6]；取消影响可恢复。
写pending场景[8,6,6,4]MiB/s、每次512KiB/s，两维逐一执行并验证取消。
这些是既有Go调度测试的运行证据，Rust只复现对应数值代数，不迁移调度策略。

源码推导扩展契约（与运行输出分开）：Influence.Loads是可变长切片；首次非零加权
按输入长度分配，None/零权重不创建pending；后续较短切片只更新前缀，较长越界panic。
StoreLoad四维byte/key/query/CPU；读region索引[0,1,2,6]，写[3,4,5,7]。
未知RW只改变hot_peer_count，不索引loads。Current保留历史，Future与所有派生值历史为空。
Min/Max逐维并独立比较peer count；Pending peer count恒0，Diff为绝对peer差。
输入快照不变；Rust Current历史深拷贝，避免Go切片别名泄露。
极值遵循Go math.Min/Max的NaN、无穷与正负零规则，不能直接用Rust忽略NaN的min/max。

## 实现及测试驱动证据

lib.rs显式登记load_values与独立load_values_test；生产实现与相邻
[行为说明](../../../pkg/statistics/load_values.rs.md)覆盖影响累计、预测值和逐维极值。
最小可编译接口后运行过滤器，[红灯](task13/red.out)：退出101，6项真实行为全部失败。
例：读future实际[0,0,0,0]、期望[101,18,33,47]；min实际[0,0,0,0]、
期望[100,10,30,-4]；负零bit实际0、期望9223372036854775808；pending累计为空。
随后实现计算，无mock结果。复用既有Loads、HistoryLoads和StoreKindLoads；无manifest/lock变更。

新增6项行为测试涵盖正负/零权重、抵消、短切片前缀、越界panic、读写及未知RW、
CPU维度、历史深拷贝与输入不变、Expect/Stddev零值、独立peer count、相等极值和IEEE特殊值。
Go pending序列按byte/key与read/write四组合检查源/目标Min/Max/Pending/Diff；
write组合及精确CPU/IEEE/越界断言属于源码推导扩展，不冒称Go新增运行快照。
预期panic使用catch_unwind验证；nocapture中的两次panic输出不代表测试失败。
Go1.26.8标准库math/dim.go只读核验Min/Max的无穷、NaN和负零契约。

## 资源和只读约束

[槽位包装器](task13/slot.sh)按1–10原子mkdir动态申请，各阶段实际均领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划、任务、会话、PID；正常/失败/信号退出先终止并wait未退出子进程组，
再清理自有owner和lock，编译缓存保留。无target-dir覆盖。
所有Cargo和槽位子进程继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
Rust修改后先cargo fmt --all；无范围外Rust格式化差异；两个指定plan保持只读。

审查proto/build.rs：只读固定模块ZIP并校验SHA256，在槽位OUT_DIR解压与生成。
审查既有Etcd夹具及scripts/rust-etcd-fixture/main.go：只读go build到槽位，
WAL/ready/log在槽位；EOF关闭，Drop等待，必要时kill后wait；无failpoint/generate。
本任务纯同步值运算，无后台任务、时钟、存储或网络；不引入本任务cancel/join和后端依赖。
workspace回归继续覆盖既有真实MemoryStore与EtcdMetaStore。

## 最终验证

[验证脚本](task13/validate.sh)、[完整日志](task13/validation.out)及
[metadata](task13/metadata.out)保留确切命令和退出码。实际使用槽位1，绝对路径如上。

| 命令 | 退出码 / 有效测试 |
| --- | --- |
| 上述既有Go oracle命令 | 0 / 3 passed |
| 同Go命令增加覆盖参数 | 0 / 3 passed |
| `cargo fmt --all` | 0 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_13_load_values -- --nocapture` | 0 / 6 passed，0 failed，0 ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0 / 26 passed，0 failed，0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0 / 1041 passed，0 failed，1既有ignored |
| `python3 docs/rust/state/task13/readonly.py` | 0 / 1101个Go/模块文件集合及SHA256一致，文档链接及diff检查通过 |

workspace忽略项是既有tso_processes，不计本任务证据；真实MemoryStore/EtcdMetaStore
测试通过。[资源核对](task13/resources.out)确认槽位1二进制与fixture进程均已退出，
锁及owner已释放，保留编译缓存。

[起点Go/模块哈希](task13/go-before.json)、[起点状态](task13/go-status-before.out)和
[最终核对](task13/readonly.out)记录起点与终点一致。aggregate SHA256为
`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
包含未跟踪文件；无新增Go文件或Go/模块差异。未修改两份指定只读plan，
用户已有其他计划变更不纳入本任务提交。

状态：已完成，无待回归项；仅验收负载预测值代数边界。

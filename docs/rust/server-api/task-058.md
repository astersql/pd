# 任务058：Region统计HTTP

基准 `3a9a6518a`。任务057的扫描/排序服务、任务001主题6准入及任务004只读oracle入口已核验。Go、模块文件和plan.md只读。

## 来源与接线

- [source-inventory](source-inventory.md)任务58行：`server/api/region.go`的GetSizeHistogram/GetKeysHistogram/calBound/calHist、`server/api/stats.go`的GetRegionStatus；只读跟踪`server/cluster/cluster.go`的getRegionStats/GetRegionStatsCount/GetHotRegionStatusByRange，以及canonical统计领域实现。
- [wire-inventory](wire-inventory.md)：HTTP-107、108、125；HTTP-109 siblings已由任务055验收；原表误归58的HTTP-114/115按任务059明确目的移交range-holes/replicated，不宣称本任务完成这些接口。
- [surface-inventory](surface-inventory.md)：router.go:284/285/309。审计与限流仍由81/82验收。
- [适配](../../../pkg/server/http_region_stats.rs)、[独立测试](../../../pkg/server/http_region_stats_test.rs)、[已有HTTP入口](../../../pkg/server/http_region_scan.rs)。复用bootstrap、leader/follower读取、查询参数解析及请求取消作用域，未复制领域索引。
- 必要宿主接线扩展[ClusterReadService](../../../pkg/server/lib.rs)、[ClusterHost](../../../pkg/runtime/cluster_host.rs)和[RoleServices](../../../pkg/runtime/member_role_worker.rs)。使用canonical范围rank计数、范围扫描、RegionStats::observe、HotStat::snapshot/RegionStatInformer；不引入新依赖、不改变领域类型或Go实现。

直方图下限size=1、keys=1000，首个解码query值生效，空值采用默认，非法整数返回Go Atoi错误。负值按有符号除法向零截断，桶端点按int64 wrapping计算并按起点排序，空集合编码为`[]`。

范围统计采用真实原始query字节；包含相交Region的完整大小，非空end排除start>=end的Region。`count`和`use_hot`按参数存在启用，count优先；反向范围计数保留Go的负rank结果。JSON保留普通空统计的`{}`、仅计数的`null`及omitempty区别。stats/region默认转发至leader，显式允许follower本地处理时仍返回Go的未bootstrap错误；直方图可读取已同步follower缓存。hot使用真实异步缓存快照，筛选仅影响store分布，Region总量不变；TiKV排除tiflash但保留tiflash_compute，缺失engine标记输出tikv，并补齐已注册store的零负载。

## 行为证据

红灯命令`cargo test --locked -p astersql-pd-server --lib task_058_ -- --nocapture`退出101，实际1失败、1忽略；测试已编译运行，请求落到gRPC fallback，Content-Type为application/grpc而非JSON。初始测试辅助函数类型错误先修正后取得该断言证据，不以编译错误作为缺陷证据。

真实TCP/tonic bootstrap、注册三种engine的store、心跳建立三个连续Region。完整JSON/HTTP状态/Content-Type差分覆盖初始无leader Region、默认/自定义/下限/极限bound、重复/空/正号/非法转义/分号参数、正常/部分/等边界/反向范围、原始二进制键、count存在性与优先级、use_hot和engine过滤、零负载与真实热写。

8次带60秒上报区间的真实心跳进入canonical hot worker，等待可观察缓存结果而非固定睡眠；Go/Rust均得到1048576 bytes/s和100 keys/s。时间区间是请求数据，不推进真实时钟。所有响应直接逐字段比较，无排序、字段删除或其他归一化。

Rust独立边界测试使用canonical Region验证空直方图、负keys、i64极值、桶end溢出和已取消处理。真实etcd场景验证hist-size的follower转发/同步后允许本地读取、真实事务阻塞时请求等待、宿主取消后的HTTP 500及服务shutdown；统计路由共用该取消/转发包装。成功读取和取消前后逐entry比较MetaStore的raft前缀，确认无额外持久写入。无新增worker。

测试开发中曾错误等待成功心跳的主动响应；该协议只在有调度响应时推送，已改为有超时的GetRegionByID观察屏障，原测试/fixture进程均终止回收。另修正了测试fixture未开启placement rules和测试误把i64::MIN当越界的断言。原日志保留，不将这些试验计为成功。

[只读监督入口](../../../scripts/http-region-stats-oracle.py)复用任务004的Go守卫、独立进程组清理和精确测试计数，显式运行默认忽略的真实Go差分。已有Go二进制为bin/pd-server，构建信息没有VCS revision，不声称它对应当前HEAD；Rust外部etcd不代表Go embedded-etcd拓扑等价。

## 验证

实际动态原子领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。默认features，`CARGO_BUILD_JOBS=1 GOMAXPROCS=2 RUST_TEST_THREADS=4`，代码变更后先运行cargo fmt --all。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_058_ -- --nocapture` | 0 | 3通过、0失败、1忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-region-stats-oracle.py` | 0 | 1通过、0失败、0忽略；39组完整HTTP响应/实现 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 203通过、0失败、19忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2098通过、0失败、56忽略 |
| `git diff --check`、Python AST及本文链接检查 | 0 | 静态检查 |

包级命令在补齐follower的clusterRouter分支后再次运行通过，聚焦及差分同样在最终源码重跑；workspace也验证最终源码。默认忽略项不计成功，任务58差分已显式运行。

状态：已完成。开始/结束Go只读守卫覆盖1101个文件（含ignored/untracked），存在性、逐文件SHA-256及Git状态完全一致；plan.md逐字节不变。冻结的1264个Rust/Cargo文件在验证后再次校验一致。所有验证进程和服务已结束，自己的槽位1占用记录及锁已清理，保留缓存；测试开发中被终止的fixture数据目录也已删除。

原始红灯、最终聚焦/差分/包级及workspace日志、确切命令/退出码/有效测试计数、Go二进制构建信息与哈希、只读守卫和源码哈希见[压缩证据](task-058-evidence.json.gz)。SHA-256 `c6b0cd52075bce30a8e49c245410d0a9e8d962aa11784af3790086eacbf9fa37`。

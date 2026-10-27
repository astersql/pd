# 任务123：热点与桶管理查询

依赖122已核实：提交 `e1b519812`、[前置证据](task-122.md)的95项场景、5项聚焦测试、
24项包测试和1796项workspace测试，以及实际Coordinator、配置和统计API均存在。
两份plan.md只读。oracle-index的123条目为source_assertion，没有冻结Go运行输出；
GetStore/GetStores索引归属124，但本任务明确要求这些入口，现作为最小支撑先提供。
后继任务应复用本canonical实现，不复制状态。

用户已授权以只读Go源码/测试推导与Rust真实行为/故障验证替代缺失的Go运行oracle。
本任务没有运行Go测试或Go oracle，没有声称Go运行差分。
[provenance](logs/task_123/provenance.json)保存来源commit、checkout commit、源码与fixture
SHA-256及来源基准比对；Go二进制/输出为null。
只读参考handler.go:949–1091、coordinator.go、core/store.go、statistics/buckets及调用方
`tests/server/api/hot_status_test.go::checkGetHotStore`；该Go测试只断言成功，不覆盖完整
字段转换，不能冒充本任务所有案例的运行证据。

生产路径：[hot_query.rs](../../../pkg/schedule/handler/hot_query.rs)。
API为 `Handler::get_store/get_stores/get_hot_regions/get_hot_stores/get_stores_loads/get_hot_buckets`；
类型为HotStoreStats、HotBucketsItem、HotBucketsResponse、HotQueryError。
[领域说明](../../../pkg/schedule/handler/hot_query.rs.md)。复用真实BasicCluster、
Coordinator、StoresStats/StatisticsSnapshot、BucketWorker和BucketRangeIndex；没有新增
私有缓存/runtime、server/proto依赖或mock。统计快照由宿主按请求提供，桶查询在真实
FIFO worker获取当前状态。独立读取不承诺跨cache原子快照。JSON保留Go字段名、
omitempty、unknown store的null值、大写hex和完整u64；只对Go未指定的map/store顺序
采用确定性map或按集合观察，错误和数值没有忽略/归一化。

独立[fixture](fixtures/task_123.json)从Go字段tag和索引手动冻结；其它字面期望在独立
[测试](../../../pkg/schedule/handler/task_123_test.rs)。测试实现不生成自己的期望。
6项真实测试，46次结果断言执行（表行复用计入），全通过、无忽略：

| 测试 | 结果断言 | 行为与边界 |
| --- | ---: | --- |
| task_123_behavior_cases | 9 | 5种store/engine表行、身份、缺失ID、所有7个JSON字段、无负载/compute/孤立统计排除、删除可见性、独立快照 |
| task_123_current_loads_and_hot_regions | 16 | 真实StoresStats前后更新、read/write两表行、缺region、重复/未知store、null、复用聚合、更新/清空快照 |
| task_123_buckets_and_faults | 8 | 真实报告入队、degree2排除/degree3纳入、全量/指定/重复/未知region、取消错误、错误后数据仍在、worker关闭错误 |
| task_123_unbootstrapped_and_empty | 9 | 六入口bootstrap优先错误、空store列表/负载、omitempty空JSON |
| task_123_bucket_snapshot_boundaries | 1 | 三bucket表行degree2/3/4、保序、空key/无限end、u64::MAX、QPS不误当写bytes |
| task_123_wide_threshold | 3 | 阈值3、2^32、u64::MAX经现有provider转i64；不截断回绕，保留负阈值语义 |

查询本身没有MetaStore写入和持久化步骤；本任务未伪造存储I/O故障。使用真实worker
的取消/关闭以及未初始化/缺store验证错误，全部worker及TTL清理均等待shutdown。

[initial](logs/task_123/initial.json)记录locked在新增本地依赖前的失败；
[build](logs/task_123/build.json)、[first](logs/task_123/first.json)、
[corrected](logs/task_123/corrected.json)记录API装配及fixture修正过程，不能视为实现红灯。
修正的fixture：Go engine值区分大小写；无限end-key继承保留旧Go字节比较，真实报告
场景改用有限end；无限end转换独立用真实canonical index快照验证。
离线重生成锁后恢复无关pulldown-cmark版本，最终仅新增已有workspace路径依赖和serde。
[behavior-red](logs/task_123/behavior-red.json)记录故意将TiFlash写bytes错误取store维度、
桶write_bytes错误取QPS后的真实失败：exit101，2通过/3失败。相同fixture检出了字段
偏差，随后恢复正确实现。[expanded](logs/task_123/expanded.json)为早期5项全通过证据。
[threshold-red](logs/task_123/threshold-red.json)为新增高阈值测试的真实红灯：
exit101，5通过/1失败，阈值4294967296错误返回1个桶，期望0；修复后再跑完整验证。
worker接受i32阈值，handler先饱和转换，仍等待真实worker结果/错误；若请求阈值高于
i32最大值则返回空map，避免整数截断。负阈值沿用既有shared config的i64语义。

最终命令及计数见[final.json](logs/task_123/final.json)；所有原始stdout/stderr以gzip
保存，记录命令、退出码、test result和日志哈希。所有阶段以原子mkdir动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[verify.py](logs/task_123/verify.py)记录持有者任务/会话/PID，并在正常、异常、中断时
等待/终止自己的子进程后清理owner及锁目录，保留缓存。设置CARGO_BUILD_JOBS=4、
RUST_TEST_THREADS=4、GOFLAGS=-mod=readonly；未运行failpoint或任何Go写入命令。

[只读复核](logs/task_123/verification.json)：1101份Go/模块文件路径与内容哈希逐一
等同起始值，含未跟踪扫描；Git无Go/模块差异，两份plan.md哈希不变，git diff --check
通过。无其它任务的实现或状态变更被纳入提交；其它任务既有删除保留。

| 最终命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 检查通过 |
| `cargo test -p astersql-pd-schedule-handler --locked --lib task_123 -- --nocapture` | 0 | 6通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-handler --locked` | 0 | 30通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查通过 |
| `cargo fmt --all -- --check` | 0 | 检查通过 |
| `cargo check --workspace --locked` | 0 | 检查通过 |
| `cargo test --workspace --locked` | 0 | 1802通过、0失败、1忽略 |

workspace的1项ignored为既有测试，1802项通过包括2项compile-fail文档测试。
仅有既有server deprecated字段warning。所有自己的槽位锁已释放，缓存保留。
相对链接及暂存差异已复核。状态：已完成。

# 任务10：存储节点滚动统计

实现前 HEAD `3c7fabb3adaa89be9e653a59c57926347b1b1b35`；Go 基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，store.go/store_test.go 与基准无差异。
已读取两份只读 plan、根规则、Copilot、ADR、依赖基线、移植指南；无更深规则。
批次9通过证据见[task-09.md](task-09.md)，累计依赖1–9满足。

## 实现前 Go oracle

`GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^TestStoreReadCPUUsesTimeMedian$' -count=1 -v`
退出0，1 passed，见[原始输出](task10/go-oracle.out)。选择的既有测试无需failpoint：
newRollingStoreStats → Observe → core query getters、StoreReadCPUUsage、movingaverage
和 GetLoad/GetInstantLoad，全程为内存操作，无注入点、生成器或模块写入。
store.go、cpu.go、movingaverage、statistics/utils 无failpoint；core 的两个注入点在
RegionInfo buckets/RegionsInfo subtree 路径，本测试不经过这些路径。
metrics init 仅注册内存指标；既有 TestMain 包装 goleak，WaitForEtcdConnections 仅检查
goroutine/必要时等待，不创建etcd、不改Go文件。go test不运行generate；readonly/off
固定模块且禁止下载。未运行make或failpoint启停。

既有测试真实断言：依次报告5、10、20、10、5秒，unified-read-pool-0=100，最终
READ_CPU平滑和瞬时值均100。无随机/时间归一化；输出为通过断言，非逐步打印快照。

## 源码推导契约（不冒称新增Go运行差分）

store.go 的7个时间维度按10秒平均窗口、5个中位数样本；CPU先乘Duration.Seconds
再加入TimeMedian，避免按报告长度稀释。Bytes/keys/query输入为区间增量，输出每秒。
CPU总量窗口5，disk窗口3，region写率窗口9；输入已为速率/用量，不除间隔。
READ_CPU近期最大值保留18个报告。collect按uint64溢出求和，再转f64。
Set遇零秒完全忽略；非零Set替换中位数/最大值，但TimeMedian.Set保留时间平均和
instant历史。Observe零间隔不增加时间平均，已满时仍可重复采样，同时更新其他维度。
间隔为unsigned timestamp差再转signed纳秒Duration，保留Go wrapping语义；Set则用
unsigned秒差。缺interval等价零。未知kind读取零。region写率是独立输入，不受心跳Set影响。

复用core::StoreStats/QueryStats/RecordPair，不新建协议报告；复用真实movingaverage
TimeMedian/MedianFilter/MaxFilter和statistics-utils::StoreLoadKind/constants。

## Rust 实现与红绿证据

`store_rolling.rs` 公开 RollingStoreStats 与 region 观测周期/窗口常量；
`observe`、`set`、`observe_regions_stats`、`set_regions_stats`、`get_load`、
`get_instant_load`、`get_read_cpu_recent_max` 对应单节点契约。使用独占借用而不是
内嵌Go互斥锁，未来共享容器负责同步。没有后台任务、宿主运行时、存储或网络。
生产文件带AsterSQL标记和[相邻行为说明](../../../pkg/statistics/store_rolling.rs.md)，
lib.rs显式登记独立测试文件。仅增加已存在canonical本地依赖；Cargo.lock只增加
statistics条目的movingaverage/statistics-utils引用，未改变任何版本或feature。

最小可编译零返回接口经fmt后执行聚焦命令，[红灯](task10/red.out)退出101，
0 passed/7 failed/0 ignored：既有CPU场景实际0期望100、所有维度首个读取实际0期望10、
Set实际0期望20、瞬时未满窗口实际0期望10、disk窗口实际0期望90、
unsigned Set实际0期望1、零间隔保护场景实际0期望10。
接入真实算法后同一命令[绿灯](task10/green.out)退出0，7 passed/0 failed/0 ignored。

新增Rust契约快照（除首项外是源码推导断言，不是新增Go运行事件）：

| 输入序列 | 关键输出 |
| --- | --- |
| Go既有CPU输入，间隔5/10/20/10/5秒，CPU=100 | 平滑100，瞬时100 |
| 同间隔，bytes/keys/query固定每秒速率10/20/30/40/50/60，CPU=70+10，disk=40+50/100，region=110/120 | 12维平滑与瞬时10/20/30/40/50/60/80/90/100/110/120/70 |
| 5秒rate10、5秒rate30、20秒rate50 | READ_BYTES平滑0→20→35，瞬时10→30→50；CPU最后平滑245、瞬时350 |
| 再5次10秒rate90 | READ_BYTES中位数90、READ_CPU630，旧窗口淘汰 |
| 新容器Set(rate20)、Observe5秒rate10、Set(rate30)、Observe5秒rate50 | 时间维度平滑20→20→30→30，瞬时0→10→10→50 |
| 10秒有效报告后零秒Set | 所有维度均保持，region独立 |
| 随后缺interval Observe，CPU u64::MAX+81，disk u64::MAX+101 | CPU汇总80，disk瞬时100/平滑95，时间瞬时保持，近期CPU最大80 |
| 初始CPU700，随后17次CPU70，再一次70 | 18报告窗口最大700→70 |
| 1至10递增输入 | CPU窗口5中位数640，disk窗口3中位数810，region窗口9中位数6 |
| region Set300/400，再Observe100/200 | 平滑200/300，瞬时100/200；心跳Set保留region值 |
| start1/end0，bytes=u64::MAX | Set unsigned秒差得1；Observe signed Duration=-1秒，瞬时=-u64::MAX，平滑保留1 |

所有12维空容器读取零；负数/越界kind读取零。QueryStats复用core的读写计数聚合，
空query为零，CPU前缀复用批次9。测试未mock任何统计结果。

## 构建资源与只读边界

全部Cargo调用由[槽位包装器](task10/slot.sh)原子mkdir动态领取，实际均为槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录plan/task/session/PID；finally在命令退出或信号中断时等待或终止并wait
命令组，再清理自身owner/锁，保留缓存。无target-dir覆盖，Rust修改后先cargo fmt --all，
没有范围外Rust格式化差异。Cargo.lock只在源码阶段串行修改。

已审查pkg/proto/build.rs：只读固定ZIP并验SHA256，解压/生成仅在槽位OUT_DIR。
workspace Etcd夹具pkg/storage/kv/etcd_fixture_test.rs只用既有go build输出槽位二进制；
scripts/rust-etcd-fixture/main.go的WAL、日志、ready写槽位目录，stdin EOF关闭，
Drop等待或必要时kill/wait。无generate或failpoint。本阶段子进程继承
GOARCH=arm64/CGO_ENABLED=0/GOFLAGS=-mod=readonly/GOPROXY=off。
本任务是纯内存单节点容器，无需后端装配；workspace回归仍覆盖真实MemoryStore和EtcdMetaStore。

## 最终验证

[验证脚本](task10/validate.sh)、[完整日志](task10/validation.out)和
[metadata](task10/metadata.out)保留确切命令及退出码。所有Cargo命令持有上述槽位。

| 命令 | 退出码与有效结果 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo metadata --format-version 1 --no-deps` | 0，statistics只有core/movingaverage/statistics-utils依赖 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_10_store_rolling -- --nocapture` | 0，7 passed/0 failed/0 ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0，12 passed/0 failed/0 ignored |
| `cargo check --workspace --locked` | 0，仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0，1027 passed/0 failed/1既有ignored |
| `python3 docs/rust/state/task10/readonly.py` | 0，1101个Go/模块文件集合、SHA256和干净Git状态一致，链接及diff检查通过 |

workspace既有ignored为tso_processes；本任务7项行为全部执行且无忽略。
既有环境不足时提前返回的健康探针不作为本任务验收证据。
Go/模块[起点逐文件哈希](task10/go-before.json)与[最终核对](task10/readonly.out)
的aggregate SHA256均为 `ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
[只读检查脚本](task10/readonly.py)验证包括未跟踪文件在内的文件集合，没有新增或改写Go。
两个指定plan.md未修改，用户已有计划变更未纳入本任务提交。

[资源核对](task10/resources.out)：Cargo和夹具子进程均退出；ps无槽位1运行产物，
自己的owner/slot-1.lock已释放，缓存保留。日志只规范化行尾空白。

状态：已完成，无待回归项。只验收单节点滚动统计，不代表整个统计主题完成。

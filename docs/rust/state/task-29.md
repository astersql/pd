# 任务29：统计容器组合与只读快照

开始 HEAD `e45bc93109c46091bc1401d95559ce176a780de6`；依赖 task-28 累计1–28证据齐备。
已读执行技能、两份只读plan、根AGENTS/Copilot、ADR、依赖基线和移植指南，无更深AGENTS。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`。

## 实现前 oracle

只读审查 hot_stat.go、slow_stat.go、store_stat_informer.go、region_stat_informer.go，
及 server/cluster/cluster_test.go 的 TestCollectMetrics、mockcluster、Go热点查询调用链。
复用 task25 的 cluster-oracle25.test（task28记录SHA256），不新增Go工具/编译。
所选 TestCollectMetrics 路径只使用内存cluster/统计与可取消调度任务；复用task25–28
已审查的TestMain/WaitForEtcdConnections；无需failpoint，不写Go/模块。
[调试输入](task29/oracle.commands)、[入口](task29/oracle.sh)、[实际输出](task29/oracle.out)：
Go真实HotStat有非nil HotCache、StoresStats、HotBucketCache；SlowStat有独立非nil缓存。
rollingStoresStats长度0，GetStoresLoads返回空map；continue后原测试PASS、退出0，
后台任务context取消退出。此运行证明组合与空态；未宣称Go具备全容器原子快照或join API。
报告后字段与转换运行证据复用task11 store、task23 hot worker、task08 bucket与task26 slow；
它们均已在累计依赖中验收，不重复实现领域计算。

源码契约：Go HotStat嵌入三个真实缓存，SlowStat独立；store值为StoreLoadKind顺序的速率，
hot get_load为去噪/四舍五入每秒速率，bucket输入period为毫秒、输出interval为秒；
慢节点重复true会删除。Rust快照为隔离owned值，各组件在FIFO查询点读取；
并发写入时不承诺跨组件事务时间点。统一cancel/join为Rust生命周期增强。

## 实现与测试驱动

新增 hot_stat.rs、独立hot_stat_test.rs、相邻说明及lib显式注册，导出HotStat、
StatisticsSnapshot、StatisticsSnapshotError。快照实现两个既有同步informer；
get_hot_peer_stats和is_region_hot保留各自过滤语义，SlowStat仅公开非空快照。
增加两个既有workspace依赖statistics-buckets与tokio-util；Cargo.lock仅增加对应依赖边。
没有新的调度、protobuf、持久化、watch或私有runtime。

最小可编译snapshot先返回空值，[红灯](task29/red.out)实际2测试，0 passed/2 failed，
退出101：报告后store不存在、关闭后仍错误返回成功。三个早期编译探测记录compile-*.out，
不计行为失败证据。接入三个真实worker查询与store/slow复制后[首轮绿灯](task29/green-initial.out)
2 passed。HotTask::Snapshot用原FIFO返回全部owned peers，避免Stats热度筛选丢失直接informer值。

补充中断shutdown回归，[第二次红灯](task29/red-interrupted.out)1 failed，退出101，
断言“interrupted shutdown lost hot worker joins”。原HotCache在await前take掉两个handle，
丢弃future会detach；改为逐个保留handle并保存已完成cache，任何worker错误仍join另一个，
future取消后可以重试。该局部依赖修复是本容器统一退出所必需，无新后台任务。

[扩展绿灯](task29/green-expanded.out)7 passed，0 failed/ignored：

- 真实StoreStats输入100字节/10秒→10 bytes/s，真实读写HotTask连续6个60秒报告，
  1048576 bytes/s去噪快照，真实bucket 1000ms报告→100000 bytes/s及owned范围key；
  slow true写入与重复true删除、双informer与hot查询、修改快照不影响下一次读取，
  后续store删除/关闭不改变已有快照。
- 组合空态（对应Go实际运行）、Registry冲突不影响原容器；bucket region/min_degree筛选；
  raw informer保留cold/anti未满peer，热度列表单独筛选，isHot依赖当前region成员。
- 有界队列Full明确报错、不返回部分快照；已挂起查询future丢弃后新查询仍完成；
  正在等待快照时cancel返回错误；待处理hot任务Canceled、bucket客户端Closed，
  三个worker统一join、重复关闭、shutdown future中断后保留hot join并完成退出。

所有成功报告路径使用真实StoresStats/HotCache/BucketWorker领域计算；没有业务结果mock。
filter投影边界另用显式owned peer值验证，时序用current-thread runtime单次poll控制。
本任务没有后端I/O；MemoryStore/EtcdMetaStore由workspace既有真实契约回归覆盖。

## 最终验证与退出

所有Cargo调用和Go二进制运行都经[动态槽位入口](task29/slot.sh)领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁中记录plan/task/session/PID；正常、失败和信号路径结束并wait子进程后释放自己的锁，保留缓存。
环境继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
已复查proto/build.rs只读固定Go ZIP缓存，在OUT_DIR生成Rust；
etcd_fixture_test.rs及既有scripts/rust-etcd-fixture/main.go调用链只读编译已有Go，
输出及隔离数据目录位于领取槽位，未调用failpoint，EOF/Drop关闭并wait进程。
本任务无新的持久化/监听路径；workspace确实运行MemoryStore与EtcdMetaStore契约。

最终入口：`bash docs/rust/state/task29/slot.sh python3 docs/rust/state/task29/validate.py`。
[逐命令汇总](task29/validation.out)、[结构化结果](task29/summary.json)和validation-N.out
记录确切命令、退出码与有效测试数。输出文件只去除行末空白，不改变断言、数值或结果。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务Rust文件产生差异 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_29_hot_stat -- --nocapture` | 0 | 7 passed、0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0 | 124 passed、0 failed/ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 非测试 |
| `cargo fmt --all -- --check` | 0 | 非测试 |
| `cargo check --workspace --locked` | 0 | 非测试 |
| `cargo test --workspace --locked` | 0 | 1139 passed、0 failed、1既有ignored |

既有ignored为tso_processes；本任务7个行为测试均无忽略。check只有既有cpu_usage弃用警告。
[只读核验](task29/readonly.out)：1101个Go/模块文件成员、SHA256及Git状态与起点一致，
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
起点go-before.json、go-status-before.out及readonly-plans.json保留；所有只读plan.md哈希不变，
相对文档链接有效，工作区与暂存diff检查通过。[资源核验](task29/resources.out)确认槽位锁释放、
缓存保留及无剩余本槽位产物进程。用户原有计划变更不纳入提交。

状态：已完成。仅验收批次29，不代表整个统计主题完成。

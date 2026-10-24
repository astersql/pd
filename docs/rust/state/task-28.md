# 任务28：统计指标注册与清理

起点 HEAD `e09c4c8481375af808b6d9adef6f6b64ceb634da`；依赖 [task-27.md](task-27.md)
已有累计1–27验收。已读执行技能、两份只读plan、AGENTS/Copilot、ADR/移植指南/依赖基线；
只有根AGENTS，无更深规则。Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，
metrics.go、store_collection.go/test.go 无后续差异。

## 实现前 oracle

复用 task27 的 statistics-oracle27.test 和 task25 的 cluster-oracle25.test；不新增或编译Go。
所选两个既有测试仅使用内存配置/StoreStatisticsMap/Prometheus；TestMain/goleak/
WaitForEtcdConnections 不注入failpoint，不改Go/模块。
[命令](task28/oracle.sh)、[运行结果](task28/oracle.out)：
TestResetStoreStatisticsClearsPlacementStatusGauge 与
TestStoreLimitMetricsIncludeTransferLeaderIn 两个测试 PASS。
placement原序列已删除，transfer-leader-in=30 后清理，均由真实Go断言证明。

复用 task25/26 已审查的 TestCollectMetrics 调用链（内存cluster、调度goroutine，
测试自己的cleanup取消并join；无failpoint/持久化/Go文件写入），Delve只改变进程内存。
[输入](task28/cluster.commands)、[输出](task28/cluster.out)、[入口](task28/cluster.sh)。
现有统计二进制未链接未使用的Reset符号，早期探测失败见 task28/oracle-linker-error.out；
改用已链接真实Reset的cluster二进制后取得证据。失败探测不计验收。

源码契约：10个metric family的name/help/type/labels逐项来自metrics.go；
异常时长使用秒和30个指数桶（1 * 1.4^i）；store限额为配置原值；hot阈值为速率，
store标签为store-N，其他store指标为十进制ID。
ResetStoreStatistics删除旧地址/任意placement标签、两种engine的六项资源与三个limit，
保留12项生命周期；DeleteClusterStatusMetrics只删除指定engine的12项状态。
region reset把10个缓存gauge置零，label/hot reset删除序列；全局Reset保留config、
hot peer summary和histogram。上述源码推导与运行结果分开，不声称额外Go运行差分。

最终Go快照：hot由15条清为0；miss-peer输入7由Reset清为0；独立label isolation
既有测试在真实LabelStatistics.Collect后1条清为0，原测试PASS。
[label命令](task28/label.commands)、[脚本](task28/label.sh)、[输出](task28/label.out)。
label测试只读内存region/store配置，后续原有重叠心跳无需failpoint；context取消释放资源。
Delve尝试写入未分配Swiss map失败，不计证据，保留task28/cluster-label-map-error.out；
最终使用已有非空label测试，不手造Go数据结构。
[fixture](../../../tests/rust-compat/fixtures/state/task-28.json)只保存有原始输入/输出支持的计数，
[binary SHA256](task28/oracle-binaries.json)固定复用产物；本次没有Go编译或新Go工具。

## 实现与测试驱动证据

新增metrics.rs、metrics_test.rs和相邻行为文档，lib显式登记/导出
StatisticsMetrics/HotMetricsSnapshot。扩展原HotPeerCache动作计数、阈值快照长度及
HotTask::Metrics/HotCache::collect_metrics；复用原worker、真实TopN与GC。
metricutil Rust目前仅MetricConfig，无注册/标签工具可调用；直接复用workspace固定的
prometheus 0.14.0，不增加第二版本、配置开关、protobuf、MetaStore或调度反向依赖。
Cargo.lock仅为既有statistics crate增加prometheus依赖边；离线重建锁时产生的无关
pulldown-cmark升级已恢复原固定版本，最终无其他依赖漂移。

[初始红灯](task28/red.out)：最小可编译接口下3个实际测试，1 passed/2 failed；
断言placement series survived reset、region预期0实际7，退出101。
[初始绿灯](task28/green-initial.out)：3 passed，随后扩展到11个真实领域/故障边界测试，
[首轮完整汇总](task28/initial-summary.json)记录聚焦11、包级116、workspace1131通过。
复查补充丢弃已完成采集结果场景，[第二次红灯](task28/red-discarded.out)实际执行1个测试，
0 passed/1 failed，断言dropped collection lost store-label cleanup，退出101。
改用完整方向快照核对Registry，不依赖一次性删除通知；
[扩展绿灯](task28/green-discarded.out)12 passed/0 failed/0 ignored。

最终12个测试覆盖：

- Go原placement清理/transfer-leader-in=30及旧/新地址、多label name、两种engine资源，
  幂等reset与不影响其他store；指定engine的状态删除与全Reset保留config/summary/histogram。
- 两个独立Registry、重复注册错误、注册中途冲突回滚；从只读metrics.go独立解析9个
  gauge的完整name/help/label关联，核对实际gather类型、histogram 30桶与2个固定label。
- 真实StoreStatisticsMap采集、placement census重建、reset后序列重新附着；
  真实RegionStatistics/LabelStatistics、累计异常duration和StoresStats的12平滑/9瞬时指标；
  缺失历史不发布、removed即使已有历史也不重新发布。
- 真实HotPeerCache阈值、action counts、threshold-only store、GC节流精确边界、
  无新流量/显式gc清理、重新激活、unknown动作、TTL不冒充remove报告；
  真实HotCache worker采集、空闲GC、cancel/shutdown双worker join、重复关闭和Closed错误。
- 读取实际Go fixture，核对hot15→0、label1→0及region7→0；
  丢弃已完成采集结果后下次采集仍清除旧标签。

重新附着、被丢弃快照恢复等为Rust生命周期安全回归；不声称Go原缓存句柄已具备这些行为。
除fixture中的Go实际运行结果，其他边界按只读源码契约与真实Rust组件验证，未冒充新增Go差分。

## 最终验证与只读退出

全部Cargo调用通过[动态槽位包装器](task28/slot.sh)，本任务各阶段实际原子领取槽位1，
绝对`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁记录plan/task/session/PID；子进程退出后finally清理自己锁，缓存保留；
信号/失败路径先终止并wait子进程组，再释放锁。环境继承GOARCH=arm64、
CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。

验证前再次审查proto/build.rs：只下载固定版本已缓存ZIP，在OUT_DIR生成Rust；
现有etcd_fixture_test.rs仅只读编译已有scripts/rust-etcd-fixture/main.go到共享槽位，
无需failpoint，数据目录隔离，EOF/Drop kill/wait退出；未改Go/模块文件。
本行为没有新的持久化或watch；workspace真实MemoryStore/EtcdMetaStore契约仍运行。
本任务worker测试显式cancel、shutdown并join，重复shutdown返回None；无新后台任务。

最终入口：`bash docs/rust/state/task28/slot.sh python3 docs/rust/state/task28/validate.py`。
[逐命令汇总](task28/validation.out)、[结构化汇总](task28/summary.json)及各validation-N.out
保存确切命令/退出码/有效测试数量。日志只规范行末空白和末尾空行，不改变数值或断言。

| 最终命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务Rust差异 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_28_metrics -- --nocapture` | 0 | 12 passed，0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0 | 117 passed，0 failed/ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 非测试 |
| `cargo fmt --all -- --check` | 0 | 非测试 |
| `cargo check --workspace --locked` | 0 | 非测试 |
| `cargo test --workspace --locked` | 0 | 1132 passed，0 failed，1既有ignored |

ignored为既有tso_processes，本任务12个行为测试无忽略；check只有既有cpu_usage弃用警告。
[只读检查](task28/readonly.out)核对1101个Go/模块文件成员、SHA256和Git状态与起点完全相同，
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`；
起点go-before.json、go-status-before.out、readonly-plans.json均保留供复核。
所有plan.md哈希未变，Markdown相对链接有效，git diff/cached diff --check通过。
[资源检查](task28/resources.out)确认槽位1锁释放、缓存保留、无剩余槽位产物子进程。
用户既有计划删除/修改及其他未跟踪计划不纳入提交。

状态：已完成。仅验收任务28指标注册与清理，不代表整个统计主题完成。

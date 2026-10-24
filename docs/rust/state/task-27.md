# 任务27：存储状态汇总

起点HEAD `0a76af3b4`，依赖task-26累计1–26完成。已阅读两份只读plan、
AGENTS/Copilot、ADR、依赖基线及移植指南；pkg/docs无更深AGENTS。
Go基准`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，两份Go参考文件无后续差异。

## 实现前 Go oracle

审查store_collection.go、TestStoreStatistics及classic collectSchedulingMetrics、MCS collectMetrics调用方；
TestMain/WaitForEtcdConnections只运行测试并检查goroutine。
所选测试构造mockconfig默认真实配置、core.StoreInfo、StoresStats/RollingStoreStats、
内存Prometheus gauges；路径无failpoint依赖、无文件I/O，无persist调用。
PersistOptions仅Persist方法有persistFail，所选路径不调用；禁止make/注入。
Go只读编译到共享槽位，Delve只更改进程内存：[脚本](task27/oracle.sh)、
[调试输入](task27/oracle.commands)、[实际输出](task27/oracle.out)、
[解析器](task27/parse_oracle.py)、[fixture](../../../tests/rust-compat/fixtures/state/task-27.json)。
`bash docs/rust/state/task27/slot.sh bash docs/rust/state/task27/oracle.sh`退出0，
槽位1，CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1。
Go TestStoreStatistics实际PASS。新增场景只修改NodeState、SlowScore和meta.LastHeartbeat；
后者依次减30s、600s、3600s，证明slow→disconnected→unhealthy→down优先级，
down抑制low-space，removed跳过容量。时间使用纳秒，容量使用字节。
17组状态快照、18条placement=1、162条cluster样本已从真实gauge值位模式解析。
重复ID8先活跃后removed，状态覆盖而原容量保留；位置计数排除removed但不去重。
早期Delve variadic调用和错误heartbeat字段探测失败，未计为证据；保留
[早期输出](task27/oracle-variadic-error.out)。最终命令无错误，时间边界的精确纳秒
比较另按只读源码与已有core行为契约测试，不把墙钟采样声称为精确纳秒差分。
注册/清理留task28，当前仅领域样本；无持久化、watch和后台任务。

## 实现与 TDD

新增store_collection.rs、独立store_collection_test.rs与相邻行为文档；lib显式注册并导出
StoreStatistics、StoreStatisticsMap、StoreMetricSample，lib.rs.md登记入口。
全部复用canonical core/storelimit/config，未变更Cargo依赖或锁文件。
[红灯](task27/red.out)从最小可编译接口运行聚焦命令，退出101，0 passed/6 failed，
包括Go STORE 0预期up/preparing=1而实际0、SLOW快照不符及样本键缺失。
实现后首轮6个测试通过；补充一个现有core低空间/limiter接线回归测试，最终
[聚焦输出](task27/validation-1.out)7 passed/0 failed/0 ignored，退出0。

七个测试覆盖Go全量状态/cluster/placement差分、健康优先级、未来heartbeat与
20秒/600秒/配置max-down的精确纳秒比较、lower max-down优先、生命周期、
removed后不覆盖旧容量/leader计数、新容器不产生removed资源样本、未知状态，
以及两个独立配置/容器、重复观察、unknown标签、独立返回样本、u64最大ID。
另验证core空间零容量/严格阈值、99/100 region与8GiB豁免边界，
v1/nil不输出v2窗口，v2输出capacity=100与四级usage=0，
真实core分数与slow-trend值、独立配置值与transfer-leader-in显式限额。
生产collect的25项配置字段逐项与Go源码核对。
后几项为只读源码契约加真实Rust组件接线检查，不声称是额外Go运行差分。
样本family与标签顺序逐项核对metrics.go；Registry、HotStat及清理仍属task28。

## 验证、只读与资源退出

所有Cargo调用经[槽位包装器](task27/slot.sh)动态原子领取槽位1，
绝对`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
记录plan/task/session/PID；正常、失败、信号退出清理自己锁并保留缓存。
环境GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
重新审查pkg/proto/build.rs（固定ZIP校验、OUT_DIR生成）和etcd_fixture_test.rs：
后者只编译已有scripts/rust-etcd-fixture/main.go到槽位，数据目录隔离，
所选路径无需failpoint，不写Go/模块；EOF关闭etcd，Drop kill/wait兜底。
本行为无存储操作或后台任务，无额外cancel/join对象。

最终[逐命令输出](task27/validation.out)、[汇总](task27/summary.json)，运行入口：
`bash docs/rust/state/task27/slot.sh python3 docs/rust/state/task27/validate.py`。
修改Rust后先cargo fmt --all，格式化差异仅本任务文件。
首轮完整回归为1119 passed/0 failed/1 ignored（[记录](task27/initial-validation.out)）；
补充第七个测试后重新执行全部基线，最终数量见下表。

| 最终命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务Rust文件格式化 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_27_store_collection -- --nocapture` | 0 | 7 passed，0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0 | 105 passed，0 failed/ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 非测试 |
| `cargo fmt --all -- --check` | 0 | 非测试 |
| `cargo check --workspace --locked` | 0 | 非测试；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0 | 1120 passed，0 failed，1既有ignored |

workspace包含真实MemoryStore与EtcdMetaStore契约；既有ignored为tso_processes，
本任务7个测试均实际执行、无忽略。
[只读检查](task27/readonly.out)确认1101个Go/模块文件集合、SHA256及Git状态完全相同；
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
起点go-before.json、go-status-before.out与readonly-plans.json保留，可运行
`python3 docs/rust/state/task27/readonly.py`复核，所有只读plan.md哈希不变。
[资源检查](task27/resources.out)确认槽位1锁释放、无子进程使用产物、缓存保留。
日志仅清理行末空白及冗余末尾空行，不改变断言、数值和退出码。
用户原有计划删除/修改及其他未跟踪计划未暂存或提交。

状态：已完成。仅验收存储状态汇总，不代表整个统计主题完成。

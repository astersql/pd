# 任务26：慢节点集合状态

起点 HEAD `31c35d05f`，依赖task-25累计1–25已完成；已阅读两份只读plan、
AGENTS/Copilot、ADR、依赖基线及移植指南，无更深AGENTS。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，slow_stat.go无后续差异。

## 实现前 oracle

只读审查slow_stat.go全部方法、cluster.go加载/心跳/唤醒调用方、
scheduling_controller.go构造/移除/观察及TestAwakenStore。
复用task25已编译cluster-oracle25.test（未改Go源码），在既有TestCollectMetrics
3022行暂停，调用真实SlowStoresStats方法，仅改变进程内存。
入口prepare/newTestCluster/newTestRaftCluster、TestMain/WaitForEtcdConnections
及既有task25审查表明所选路径无failpoint要求，不写Go/模块。
[调试命令](task26/oracle.commands)、[运行输出](task26/oracle.out)、
[解析器](task26/parse_oracle.py)、[fixture](../../../tests/rust-compat/fixtures/state/task-26.json)。
动态领取槽位1运行`bash docs/rust/state/task26/slot.sh bash docs/rust/state/task26/oracle.sh`，退出0。
14快照包含初始空、首次true插入、重复true删除、false/重复remove、两个独立store、
0与u64::MAX。单位为无符号64位store ID，无时钟、错误、I/O和后台任务。
集合只在不存在且isSlow=true时插入，其他情况删除；exists严格为集合非空。
早期直接通过Go嵌入外层调用Exists的Delve接收器解析有误（输出与map矛盾），
保留oracle-promoted-receiver.out；最终全部显式SlowStoresStats接收器，14步map/exists一致。
这是实际运行快照，未将源码推导冒充差分，也不声称中断的测试自身通过。
Delve exit终止debuggee，docker --rm回收容器。

## 实现与 TDD

新增slow_stat.rs/slow_stat_test.rs与相邻说明，lib.rs显式注册生产/测试模块并导出
SlowStoresStats、SlowStat；lib.rs.md增加入口。无需Cargo依赖/锁文件变化。
最小可编译接口后[红灯](task26/red.out)：聚焦命令退出101，0 passed/4 failed；
明确断言Go step 1实际false、期望true，其他测试失败于首次slow记录。
实现RwLock<HashSet<u64>>后[绿灯](task26/validation-1.out)相同命令退出0，4 passed。
覆盖14步Go观察/移除/非空事件，重复true、false、重复remove、0/max ID、
不同store/容器隔离，以及8线程共享同store共808次true观察的最终空集合。
所有测试线程join，最后再观察一次确认非空；测试无忽略，无业务结果mock。
SlowStat公开slow_stores_stats并Deref，对应Go嵌入同一缓存；观察的判断/更新持有
同一写锁，读取持有读锁，无后台资源或后端I/O，不引入protobuf/transport/runtime。

## 完整验证与退出

所有Cargo命令经[槽位包装器](task26/slot.sh)动态原子领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
包装器记录plan/task/session/PID，正常/失败/信号退出先结束并wait进程组，再释放自己的锁；
缓存保留，无target-dir覆盖。Go环境GOARCH=arm64、CGO_ENABLED=0、
GOFLAGS=-mod=readonly、GOPROXY=off。
已重新审查pkg/proto/build.rs：固定ZIP校验、OUT_DIR生成；
etcd_fixture_test.rs调用既有scripts/rust-etcd-fixture/main.go，仅编译到槽位和写隔离
数据目录，不需failpoint，不写Go/模块，EOF关闭且Drop kill/wait兜底。

[逐命令输出](task26/validation.out)、[统计](task26/summary.json)，全部退出0：

| 命令 | 有效测试数 |
| --- | --- |
| `cargo fmt --all` | 格式差异仅本任务Rust文件 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_26_slow_stat -- --nocapture` | 4 passed、0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 98 passed、0 failed/ignored |
| `cargo metadata --format-version 1 --no-deps` | [metadata输出](task26/validation-3.out) |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 1113 passed、0 failed、1既有ignored |

workspace覆盖真实MemoryStore/EtcdMetaStore契约；既有ignored为tso_processes。
[只读检查](task26/readonly.out)：1101个Go/模块文件集合、SHA256、Git状态完全一致，
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
起点go-before.json/go-status-before.out及readonly-plans.json保存核对，
所有plan.md哈希不变，用户原有变更未暂存。
[资源检查](task26/resources.out)确认自己的锁释放，无子进程使用槽位，缓存保留。
输出日志只清理行末空白及冗余末尾空行，不改变返回值或断言。

状态：已完成；只验收慢节点集合边界，不代表整个统计主题完成。

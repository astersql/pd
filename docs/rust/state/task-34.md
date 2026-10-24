# 任务34：全量同步快照边界

实现起点 `96096b6a27112458f05022f9cbebf1522b270d9b`；依赖[任务33](task-33.md)已完成。
已读取两份只读计划、根规则、Copilot、ADR、移植指南和依赖基线，无更深AGENTS。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`。

## 实施前 Go oracle

[入口](task34/oracle.sh)、[实际输出](task34/oracle.out)：退出0，5 passed。
显式编译既有server/client/history/metrics及server/history测试，排除client_test.go的
failpoint TestMain；未运行RunServer、取消循环或注入路径。GOFLAGS=-mod=readonly、
GOPROXY=off、CGO_ENABLED=0；Linux二进制由Docker Alpine执行。未新增或修改Go。

运行断言：
- BuffersLiveRecords：history reset10，单region快照发送阻塞时写100..104，retain使2容量增长；
  快照1条，追赶start10共5条，最终history/sendIndex15；1001region快照首批1000后关闭，
  返回Unavailable且仅一帧。
- BindStreamWaitsForAppendHistoryRecordsBoundary：绑定等待外部append锁，写3条后捕获13。
- StartIndexZeroDoesNotGrowHistoryWindow：空快照/空history全量完成，容量仍2。
- EmptySnapshotCatchUpCompletion：空快照，42起1001条history，追赶帧位置0/1000，
  completion1043；发送位置不能解释为历史索引。
- FailsWhenCatchUpHistoryExceedsMax：history最大1，快照阻塞期间追加2条，返回ResourceExhausted。

源码契约（与实际运行断言区分）：先固定bind-time syncStartIndex，扫描前retain，
GetRegions为无序Arc快照；快照分帧起点为从0计数的位置，不推进history/sendIndex。
retainedRecordsFrom必须验证完整区间，defer在失败/退出时释放retain。
本任务实现领域快照、发送回调边界和retain所有权；追赶、completion、流注册和协议转换
仍由后续任务接线，不将本任务当作整个发送服务完成。

## TDD 与实现

[失败日志](task34/red.out)：最小可编译接口运行5项，退出101，1 passed / 4 failed，
包含实际断言 history end10!=15、空批次!=[(0,1000),(1000,1)]、预期发送错误未出现。
补齐真实 BasicCluster 的 Arc 扫描后，5项通过；没有修改Go或增加依赖/feature。

新增 `full_snapshot.rs`，导出 `FullSnapshot`、`SnapshotBatch`、`SnapshotOffset`，
在lib.rs显式注册生产/独立测试模块。`bind`要求宿主整个append批次与绑定共用锁；
`&mut HistoryBuffer`防止同一次绑定读取跨越可变访问。锁内固定next并retain再扫描，
返回后发送不持有锁。SnapshotOffset与HistoryIndex类型隔离，快照不会推进历史。
`send`只在真实网络边界使用回调，成功返回保留对象以衔接追赶；错误/取消错误/panic
通过所有权退出释放retain，不创建后台任务或runtime。RegionInfo使用Arc原值，保留所有字段。

测试使用真实BasicCluster/HistoryBuffer，覆盖1001条批次与自定义400批次、空快照、
冻结后cluster新增、发送期间5条历史增长、retain成功保持及最终释放、第二帧关闭错误、
panic清理、最大容量导致HistoryGap、线程绑定等待完整append后得到13；线程显式join。
Go GetRegions和Rust集合顺序均无保证，多region完整性按ID排序比较，未忽略字段或错误。
本模块无MetaStore I/O；包级测试包含既有MemoryStore与EtcdMetaStore的持久化/故障回归。
已审查etcd_fixture_test.rs与scripts/rust-etcd-fixture/main.go：仅只读go build，
进程stdin EOF退出、Drop等待，失败时kill并wait；proto/build.rs仅校验模块归档并在OUT_DIR
生成Rust。所有子进程继承readonly/off/CGO=0，禁止注入及写Go/模块文件。

## 最终验证与资源

所有阶段通过[slot.sh](task34/slot.sh)逐个原子mkdir动态领取槽位，实际均为1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划、任务、会话、PID；正常/失败/中断时等待子进程退出后释放自己的锁，保留缓存。
[资源核验](task34/resources.out)确认锁已释放、缓存保留、无fixture/oracle/test残留进程。

正式入口 `bash docs/rust/state/task34/slot.sh python3 docs/rust/state/task34/validate.py`。
命令、退出码及有效测试数见[日志](task34/validation.out)与[汇总](task34/summary.json)：

| 命令 | 退出码 | 测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 非测试 |
| `cargo test -p astersql-pd-syncer --locked --lib state_task_34_full_snapshot -- --nocapture` | 0 | 5 passed / 0 failed / 0 ignored |
| `cargo test -p astersql-pd-syncer --locked` | 0 | 38 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 非测试 |
| `cargo fmt --all -- --check` | 0 | 非测试 |
| `cargo check --workspace --locked` | 0 | 非测试 |
| `cargo test --workspace --locked` | 0 | 1177 passed / 0 failed / 1 ignored |

既有ignored为tso_processes，check仅有既有cpu_usage弃用警告。
[只读核验](task34/readonly.out)：1101个Go/模块文件的集合、SHA256和Git状态与
[起点](task34/go-before.json)完全一致；aggregate SHA256=
`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
所有plan.md与[起点](task34/plans-before.json)一致；文档链接和git diff --check通过。

状态：已完成。只验收全量快照边界，后续追赶和server协议接线仍属后续批次。

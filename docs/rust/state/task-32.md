# 任务32：同步历史索引持久化

依赖批次31已验收，起点 bb739f866a37dc2f5fc2fd441f9ab33ad007110e。
实现前只读审查 history_buffer.go/history_buffer_test.go、metrics.go、server.go、
Base/MetaStore、etcd_fixture_test.rs、proto/build.rs、ADR/移植指南及两份只读plan。
无更深AGENTS。Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0。

必要Go运行证据复用 task30/oracle.out 与 task31/oracle.out：
TestHistoryBufferRecordsAndFlushes、TestHistoryBufferPersistsNextIndexOnly 均通过。
只读入口task30/oracle.sh显式只编译已有history_buffer.go、metrics.go及测试，
不包含其他syncer测试/TestMain；相关MemoryKV与RegionInfo路径无需failpoint。
不执行make、生成器、注入或新增Go文件。

运行断言契约：record六条并persist后存储"6"；重建first=next=6,len=0,get(5)=nil；
再record 101条后next=107,first=7,records=ID1..100，存储"106"。
另一Go测试record三条persist后重建first=next=3,len=0,get(2)=nil，存储"3"。
这些是既有通过测试的显式断言，不冒充额外调试快照。

源码推导单独列示：historyIndex为Base相对键，十进制uint64；缺失/空值从0开始；
每100条尝试persist，无论写成功或失败均重新计数；reset重置倒计数；
resetAndPersist先清空内存再写；显式persist不改变倒计数。
Go损坏值log.Fatal，Rust返回CorruptIndex；Go加载错误警告后继续，Rust返回Load给宿主决策；
Go写失败警告，Rust返回携带已提交内存next的Save，不回滚记录、不终止进程。
所有故障映射来自源码审查和Rust真实后端故障测试，不声称Go故障运行差分。

## 实现与测试驱动

新生产文件 history_persist.rs 的 PersistentHistory 独占现有 HistoryBuffer，
借用真实 Base/MetaStore；HistoryPersistError 保留 History/Load/Save 源错误及 CorruptIndex。
使用现有 workspace storage-kv 依赖，dev 依赖 tokio/async-trait；仅更新 syncer 的锁条目，
没有新版本、feature、协议模型或存储平行实现。相邻 .rs.md 记录恢复与取消边界。

登记最小可编译接口后运行 fmt 与带 --locked 的聚焦命令：
[red.out](task32/red.out) 退出101，0 passed/5 failed/0 ignored。
断言失败包括存储为空而非"6"/"100"、重启next=0而非3，以及断网写未返回Save。
实现 reload/flush 后同样5项通过，见[green.out](task32/green.out)，退出0。
增加nextIndex-only、溢出、取消与窗口接线回归，最终8项聚焦和27项包级通过。

取消测试曾尝试用停机etcd制造Pending，但真实网络可立即返回Unavailable；
改用仅暂停transact的I/O边界，其他操作转发真实MemoryStore，手动poll至Pending后drop。
验证next=100和100条内存记录仍在、下条追加不重复写、恢复I/O后显式persist并重载next=101。
不模拟历史/索引结果；真实etcd停机、写失败、load失败和重启测试独立保留。
取消后没有后台任务需要join；本组件无spawn。Etcd夹具Drop关闭stdin并wait，
超时kill后wait；正常与测试失败路径均释放。成功后next=800、records空。

运行时Go基准复用两个已有成功测试的全部关键断言，MemoryStore和EtcdMetaStore
均执行同一六条/101条输入与恢复快照。另有三条persist后的first=next=3,len=0。
格式错误、load失败、写失败、取消、索引耗尽为Rust验证与明确错误映射，
不宣称这些额外场景已经获得Go故障运行差分。

## 共享资源与只读验证

全部Cargo阶段使用[slot.sh](task32/slot.sh)原子动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话/PID；正常/失败/中断等待子进程结束再清理自己的锁，缓存保留。
复核proto/build.rs在OUT_DIR解压校验过的固定Go模块ZIP并生成Rust；
etcd_fixture_test.rs仅go build现有scripts/rust-etcd-fixture/main.go，
GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0，不注入failpoint，不写Go/模块文件。

[只读核验](task32/readonly.out)确认1101个Go/模块文件集合、SHA256与Git状态均未改变，
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
起点清单为[go-before.json](task32/go-before.json)及[go-status-before.out](task32/go-status-before.out)。
所有29份plan.md对照[plans-before.json](task32/plans-before.json)内容一致。

## 最终验证

入口 `bash docs/rust/state/task32/slot.sh python3 docs/rust/state/task32/validate.py`。
锁条目更新后所有正式验证均使用 --locked；完整输出及精确命令见
[validation.out](task32/validation.out)、[summary.json](task32/summary.json)和validation-N.out。

| 命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 非测试 |
| `cargo test -p astersql-pd-syncer --locked --lib state_task_32_history_persist -- --nocapture` | 0 | 8 passed / 0 failed / 0 ignored |
| `cargo test -p astersql-pd-syncer --locked` | 0 | 27 passed / 0 failed / 0 ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 非测试 |
| `cargo fmt --all -- --check` | 0 | 非测试 |
| `cargo check --workspace --locked` | 0 | 非测试 |
| `cargo test --workspace --locked` | 0 | 1166 passed / 0 failed / 1 ignored |

既有ignored为tso_processes；check仅有既有cpu_usage弃用警告。
[资源核验](task32/resources.out)确认锁释放、缓存保留及无槽位产物进程残留。
最终复核Go/模块集合、哈希、状态与所有plan.md均未变；git diff --check与证据链接检查通过。

状态：已完成；仅验收批次32的历史索引持久化，不代表整个区域同步主题完成。

# 任务31：历史保留与自适应容量

依赖task-30已验收（累计1–30）；起点 `a85e70e945081e85055702892178922bbdb6ca69`。
已读执行技能、两份只读plan、根AGENTS/Copilot、ADR/依赖/移植指南；无更深规则。

## 实现前 oracle

Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，只读审查
pkg/syncer/history_buffer.go、history_buffer_test.go、metrics.go及server.go调用方。
复用[task30的只读入口](task30/oracle.sh)和[运行输出](task30/oracle.out)，
其中9个顶层测试和5个容量子测试通过；包含本任务的retain、observed window、
full sync priority与连续低窗口收缩测试。输入和断言来自已有测试，非新增Go oracle。
重跑结果保存在[oracle.out](task31/oracle.out)。

调用链审查复用task30：文件列表排除syncer client/server测试及TestMain；
RegionInfo构造和MemoryKV单键Load/Save路径无需failpoint；无源码/模块写入。
Go编译仅生成槽位中的linux/arm64二进制，Docker alpine只读挂载后执行并退出。
不运行make、注入、生成器、tidy；只使用既有Go源码。

契约：history index不是MetaStore revision，无时间单位。
base=2/max=8/unit=1，reset=10、retain(10)、record ID100..104后：
first=10,next=15,len=5,capacity=8，追赶成功。
reset=10，record1,2、observe(5)、record3,4,5：同样first=10,next=15,
records=[1,2,3,4,5],capacity=8；full-sync retain与observe(4)并存结果相同。
observe(3)增长到8；第一次检查消费高观察值；再连续三次低窗口检查缩到4。
这些期望来自已通过Go测试的显式断言；没有声称额外内部状态来自调试快照。

其他源码推导：observe最大值在每轮检查后清零；>capacity/4重置低窗口计数；
retain存在禁止收缩；每次最多减半且不低于base。增长按unit*2^n归一化，
max至少为base。达到max仍会覆盖，retainedRecordsFrom以ok=false报告缺口；
Rust使用HistoryGap携带requested/first/next。Rust饱和算术避免Go整数溢出，
guard Drop在失败/取消释放且reset隔离旧guard，属于Rust所有权增强测试。

## 实现与测试驱动

原有HistoryBuffer增加with_config(base, maximum, unit)及WindowPolicy；原构造保持固定容量。
生产history_window.rs实现retain_from/HistoryRetain Drop、retained_records_from、
observe_required_window/maybe_shrink，record预先按最早保留点增长；reset回到base并换代。
不新增依赖或Cargo.lock变化，不引入平行历史容器、协议、MetaStore或后台线程。

先登记最小可编译接口并执行fmt及聚焦命令；[红灯](task31/red.out)退出101，
实际0 passed/8 failed/0 ignored：容量仍为2而非8/4/16，reset后追赶返回空快照
而非[1,2,3,4,5]。实现后相同8项测试[通过](task31/green.out)，退出0。
再补4项健壮性回归，最终12项聚焦与19项包级通过，见[最终聚焦输出](task31/validation-1.out)。

覆盖Go四组验收场景、无retain增长、max缺口与未来/空区间、重复retain独立释放、
最早retain与环绕后增长、reset旧guard隔离、连续三轮低窗口/高窗口重置/逐级回base、
巨大观察值饱和处理、无法恢复已覆盖历史、索引耗尽失败保持快照。
所有记录使用真实core RegionInfo；成功路径检查容量、索引及记录顺序。

取消证据：实际poll持有guard的future到Pending，活动期间三轮检查不收缩；
drop future后连续三轮收缩。Result错误返回及跨线程drop/join也证明释放；
无mock统计/索引结果。该纯内存算法无需MemoryStore/EtcdMetaStore接线；
完整workspace回归另覆盖既有两后端，并不冒充本任务新增存储实现。


## 最终验证与资源退出

所有Cargo与oracle阶段均由[slot.sh](task31/slot.sh)原子动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
占用记录含plan/task/session/PID；正常/失败/中断均在子进程退出后清理锁，缓存保留。
本领域模块无后台任务；guard取消及跨线程join证据见测试。
复查pkg/proto/build.rs只读固定Go模块ZIP，在OUT_DIR生成Rust；
pkg/storage/kv/etcd_fixture_test.rs只读编译既有scripts/rust-etcd-fixture/main.go，
不需要failpoint；GOFLAGS=-mod=readonly、GOPROXY=off，夹具文件仅写槽位。
夹具Drop关闭stdin并wait，必要时kill后wait；无Go源码或模块写入。

最终入口：`bash docs/rust/state/task31/slot.sh python3 docs/rust/state/task31/validate.py`。
[汇总](task31/validation.out)、[结构化结果](task31/summary.json)、validation-N.out
保留确切命令、退出码和有效测试数量。

| 命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化，无无关Rust变更 |
| `cargo test -p astersql-pd-syncer --locked --lib state_task_31_history_window -- --nocapture` | 0 | 12 passed，0 failed/ignored |
| `cargo test -p astersql-pd-syncer --locked` | 0 | 19 passed，0 failed/ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 非测试 |
| `cargo fmt --all -- --check` | 0 | 非测试 |
| `cargo check --workspace --locked` | 0 | 非测试 |
| `cargo test --workspace --locked` | 0 | 1158 passed，0 failed，1既有ignored |

既有ignored为tso_processes；check仅有既有cpu_usage弃用警告。
[只读核验](task31/readonly.out)确认1101个Go/模块文件的集合、SHA256和Git状态未变，
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
[起点哈希](task31/go-before.json)、[起点状态](task31/go-status-before.out)、
[计划哈希](task31/plans-before.json)已保存；所有plan.md保持原样，链接及diff检查通过。
[资源核验](task31/resources.out)确认锁释放、缓存保留及无槽位产物进程残留。

状态：已完成；仅验收批次31领域历史保留与自适应容量，不代表整个同步主题完成。

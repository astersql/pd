# 任务17：标签隔离状态统计

开始 HEAD `e65fcfee1b4adc7c549cd6ad7d3d5d9757c7d15a`，源码基准
`69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；依赖[任务16](task-16.md)累计1–16已完成。
已阅读两份只读计划、根AGENTS、Copilot、ADR、依赖基线及移植指南，无更深AGENTS。

## 实现前 oracle

命令 `GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off go test ./pkg/statistics -run '^TestRegionLabelIsolation(Level|CaseInsensitive)$' -count=1 -v`，
退出0，2个顶层测试及11个Unicode/大小写子测试通过，见[输出](task17/go-oracle.out)。
只读审查TestMain、goleak、WaitForEtcdConnections、NewStoreInfo/SetStoreLabels、NewRegionInfo、
GetRegionLabelIsolation/IsRegionLabelIsolationSatisfied及Observe调用链：这些执行路径不需要failpoint，
不运行生成/格式化/模块整理；Go模块强制只读。没有新增或修改Go oracle。

Go原测试的7组zone/rack/host输入依次得到rack/host/zone/rack/none/rack/host，
计数none=1、host=2、rack=3、zone=1；以同一ID序列缩短labels至zone/rack，
依次rack/none/zone/rack/none/rack/none，计数none=3、host=0、rack=3、zone=1。
每组同时验证空/none/未知隔离要求和各已知层级；空stores、空labels返回none/-1，单store返回首层。
Unicode输入保持原测试的İ、ı、ς/σ、ſ/s、K/K、ß/ss及ASCII大小写与缺失值。
原断言是运行oracle；无逐事件JSON输出，不将源码分析当运行差分。

源码契约扩展：重复observe不累计；变化先减旧层级再加新层级，零计数项保留；
mark只标记，clear对当前已观察ID减一次并清空待清理集合；重复/未知mark安全，
mark之后observe不会取消清理。上述defunct边界是Go源码契约扩展测试，与原Go运行证据区别记录。
本行为为同步内存统计，无后台任务、时钟或持久化边界。

## 实现与测试驱动证据

先显式登记独立测试模块，提供最小可编译签名；
`bash docs/rust/state/task17/slot.sh bash -c 'cargo fmt --all && cargo test -p astersql-pd-statistics --locked --lib state_task_17_label_statistics -- --nocapture'`
退出101，4 failed/0 passed：[红灯](task17/red.out)。实际断言包括
("none", -1) != ("rack", 1)/("host", 2)/("zone", 0)，空计数 != {zone:2}，
不是缺符号或零测试。实现时首次误用RegionInfo::id导致编译失败，
改为现有meta().id；[编译记录](task17/initial-compile.out)单独保存，不计行为红灯。

[生产实现](../../../pkg/statistics/label_statistics.rs)与
[独立测试](../../../pkg/statistics/label_statistics_test.rs)在canonical statistics crate扩展，
公开get_region_label_isolation、is_region_label_isolation_satisfied和LabelStatistics。
依赖与锁文件不变。隔离算法复用真实StoreInfo::compare_location，对各pair首次差异取最大层级：
Go每层尚未隔离分组包含的恰好是到该层仍无已知差异的pair，故逐对算法保留缺失值语义，
且不复制缺失值跨组产生的队列。原始标签名和要求匹配的大小写语义保留。
独占可变借用串行修改index，宿主负责共享锁；独立计数快照可交给宿主指标收集器。
无全局指标注册、runtime或后台任务。

测试覆盖Go的全部7组层级/重观察输入和11组Unicode/缺失值输入，
以及空输入、单store、标签key大小写、要求key精确匹配、defunct重复/未知ID、
标记后观察换级别、重复clear、清理后重新观察、零计数保留与独立快照。
原Go测试断言运行通过后，以同输入的Rust断言比对输出；未声称有额外Go逐事件日志。

## 共享资源及只读边界

[槽位包装器](task17/slot.sh)使用原子mkdir动态领取1–10，记录plan/task/session/PID，
所有Cargo及夹具子进程继承绝对CARGO_TARGET_DIR，以及
GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
正常、失败和信号退出均在命令停止后删除自有owner/锁，缓存保留；不删除其他锁或缓存。
已审查pkg/proto/build.rs的固定ZIP下载校验与OUT_DIR生成路径；
workspace的真实Etcd夹具只读build已有scripts/rust-etcd-fixture Go源码，
二进制/数据留在所领槽位，Drop关闭stdin、必要时kill并wait，不使用failpoint或源码生成。
本任务是同步内存统计，无需MetaStore；真实MemoryStore/EtcdMetaStore由workspace既有契约回归验证。

## 最终验证

通过命令：`bash docs/rust/state/task17/slot.sh bash docs/rust/state/task17/validate.sh`。
[验证脚本](task17/validate.sh)、[完整输出](task17/validation.out)、
[metadata](task17/metadata.out)、[汇总](task17/summary.json)。

| 命令 | 退出码 / 有效测试数 |
| --- | --- |
| 上述只读Go oracle命令 | 0；2个顶层测试，11个子测试 |
| `cargo fmt --all` | 0；无范围外格式化变更 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_17_label_statistics -- --nocapture` | 0；4 passed，0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0；46 passed，0 failed/ignored |
| `cargo check --workspace --locked` | 0；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0；1061 passed，0 failed，1既有ignored |
| `python3 docs/rust/state/task17/readonly.py` | 0；Go/模块集合、SHA256、Git状态不变，链接/diff检查通过 |

既有ignored为tso_processes，不计本任务验收；真实MemoryStore/EtcdMetaStore回归通过。
红灯、初次编译修正和最终验证阶段均动态取得槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[资源检查](task17/resources.out)确认命令及使用槽位产物的进程已退出，自有锁已释放，缓存保留。
[只读检查](task17/readonly.out)、[起始哈希](task17/go-before.json)、
[起始Git状态](task17/go-status-before.out)确认1101个Go/模块文件无新增或内容/状态变化；
aggregate SHA256为`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
指定两份plan.md和来源史诗均未修改，保留原有用户改动。

状态：已完成；仅验收标签隔离统计边界。

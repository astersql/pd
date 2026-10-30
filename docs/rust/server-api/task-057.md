# 任务057：Region排序HTTP

基准 `62557be3a`；已核验任务056的真实Region视图、HTTP流编码与验证记录，以及任务001的主题6准入范围。Go、Go模块、plan.md只读。

## 来源与实现

- [source-inventory](source-inventory.md)：任务57行；`server/api/region.go` 的9个GetTop方法、getTopNRegions、RegionHeap及TopNRegions；`server/api/stats.go`本次只读核对，范围统计仍属于任务58。
- [wire-inventory](wire-inventory.md)：HTTP-089至097，writeflow、writequery、readflow、readquery、confver、version、size、keys、cpu。
- [surface-inventory](surface-inventory.md)：router.go:265–273。审计及限流仍由81/82验收，不宣称本次完成。
- [排序适配](../../../pkg/server/http_region_top.rs)、[独立测试](../../../pkg/server/http_region_top_test.rs)。局部扩展[已有扫描HTTP](../../../pkg/server/http_region_scan.rs)注册路由并复用bootstrap检查、leader/follower读服务、一次Region引用快照和流编码，避免复制装配或领域状态。

选择器使用Go的有界最小堆语义，复杂度O(n log limit)，额外堆空间O(limit)，现有视图仍持有O(n)引用快照。默认limit=16，上限10240，非正数返回空数组；参数保留Go首值、解码、Atoi及错误状态。i128比较精确保留uint64与int64范围，不通过浮点比较。query维度调用canonical计数器，CPU使用Go旧cpu_usage字段，size与keys使用已归一化值。周期性yield并检查请求取消；响应复用已有pull-driven流，无新增worker或持久化写入。

并列时不引入ID次级排序：相同输入按container/heap的左子树优先处理，值相等不替换堆顶。Go输入来自map，跨请求及实现间的同值顺序/截断身份并无保证。

## 行为证据

红灯：`cargo test --locked -p astersql-pd-server --lib task_057_ -- --nocapture`退出101，实际1失败：未注册top路由落入gRPC fallback，Content-Type为application/grpc而非JSON，非编译失败。

真实TCP/tonic bootstrap与heartbeat写入24个Region，各维度采用不同排列。所有9维验证默认/空limit、首值、正号、非正/最小负整数、超大limit、越界/非法参数、Store路径不误派发为top、完整Region JSON和Content-Type。CPU与query计数超过2^53，keys覆盖uint64到负int64转换。独立用10245个canonical Region验证10240上限，覆盖空集合、固定输入并列堆序、signed极值及已取消选择器。

[只读监督入口](../../../scripts/http-region-top-oracle.py)运行既有Go `bin/pd-server`与Rust同场景：182次HTTP观察/实现，其中173项保存完整响应并比较；另外9次并列截断验证值序列、成员合法及唯一。仅对完整列表中相同排序值的组按ID归一化，未排序不同值组；并列截断不要求任意Go map选出的身份相同。query计数不出现在Region JSON，测试按已发送心跳的Region ID恢复预期rank，并比较完整实际输出。

额外Rust真实etcd场景验证follower转发/允许本地读取、真实事务阻塞时宿主取消、HTTP终止以及Region持久状态不变。流编码及断开释放由相邻task056测试在包级回归中覆盖。Go二进制无VCS revision，不宣称对应当前HEAD；Rust外部etcd不代表Go embedded-etcd拓扑等价。

## 验证

动态原子领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。默认features，`CARGO_BUILD_JOBS=1 GOMAXPROCS=2 RUST_TEST_THREADS=4`；每次Rust修改后先运行`cargo fmt --all`。

| 命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_057_ -- --nocapture` | 0 | 5通过、0失败、1忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-region-top-oracle.py` | 0 | 1通过、0失败、0忽略；182次HTTP观察/实现 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 200通过、0失败、18忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2095通过、0失败、55忽略 |

回归风险：一次包级执行和一次独立重现中，已有`grpc_alloc_id_test::task_021_behavior`因额外128个ID而失败，退出101（分别198通过/1失败与0通过/1失败）。`pkg/runtime/schedule_worker.rs`会提前`alloc_count(128)`，与观察到的偏移一致；这是源码解释，不宣称已修复该竞态。任务57不修改分配器、调度worker或原测试；相同最终源码包级重跑200通过，保留全部失败日志。

状态：已完成。默认忽略项不计成功，任务57差分已显式单独运行。开始/结束Go守卫覆盖1101个文件（含ignored/untracked），存在性、SHA-256及Git状态完全一致；plan.md逐字节不变。最终1262个Rust/Cargo文件哈希在验证后复核一致。所有验证进程及服务均结束，自己的槽位1锁已释放，保留缓存。

原始红灯、最终差分、所有回归失败与成功日志、命令/退出码、只读守卫、Go构建信息及最终源码哈希见[task-057-evidence.json.gz](task-057-evidence.json.gz)。SHA-256 `57360cac10267856c89a63ac93623945f5989a9a04f4671cb1bd89269b3fc23f`。

# 任务059：Region异常检查HTTP

基准 `872fc8cbf`。任务058、任务001主题6准入和任务004只读oracle入口已核验；plan.md、Go文件及Go模块文件保持只读。

## 来源与实现

- [source-inventory](source-inventory.md)任务59行及checker目录来源行：复用canonical RegionStatistics、RegionsInfo及Handler::check_regions_replicated；不移植或重写checker算法。
- [wire-inventory](wire-inventory.md) HTTP-098至106、114、115；[surface-inventory](surface-inventory.md) router.go:274至282、291至292。审计和限流由任务81/82验收。
- [HTTP适配](../../../pkg/server/http_region_check.rs)、[独立测试](../../../pkg/server/http_region_check_test.rs)、[共享读取包装](../../../pkg/server/http_region_scan.rs)、[宿主读取](../../../pkg/runtime/cluster_host.rs)、[角色读取](../../../pkg/runtime/member_role_worker.rs)、[协调器队列](../../../pkg/runtime/schedule_scatter.rs)。lib.rs显式注册独立测试。
- Runtime增加已有workspace依赖astersql-pd-schedule-handler，Cargo.lock只更新runtime的依赖边；没有新增第三方依赖、feature或平行服务。

miss/extra/pending/down/learner/empty/offline/oversized/undersized读取心跳维护的分类索引，并在同一宿主锁下解析当前Region。空集合输出`{"count":0,"regions":[]}`；返回的Region字段和列表编码复用canonical response。HTTP不重算分类、不写持久状态、不新建worker。

range-holes复用canonical范围树，保留空集合、首尾空洞和大写hex键。允许已同步follower本地读取；其他异常检查及replicated属于clusterRouter，显式follower本地处理返回未bootstrap错误，普通follower请求经现有包装转发leader。

replicated经现有term调度队列串行调用canonical Handler，读取同一RuleManager、Region树和checker的rule-pending集合。JSON字符串保留REPLICATED/INPROGRESS/PENDING；Scatter重试队列不等于rule-pending，不能因加速/Scatter失败就报告PENDING。缺少startKey或endKey在bootstrap检查前返回Go一致的404文本；非法hex返回400，原始非UTF-8字节在转Rust字符串前校验，保留Go错误文本。空值、相交范围、反向范围均委托领域扫描。

## 行为证据

初始`cargo test --locked -p astersql-pd-server --lib task_059_ -- --nocapture`退出101，实际1个行为测试失败。测试已经编译运行；请求落入gRPC fallback，Content-Type为application/grpc而非JSON。没有用编译错误作为缺陷证据。

真实TCP/tonic bootstrap、注册Store、连续心跳替换同一个Region。逐字段比较79组Go/Rust HTTP状态、JSON/文本及Content-Type，包含九类异常的出现/消失、大小变化、空Region、缺失/多余peer、learner/down/pending、Store removing后的offline、空洞、REPLICATED/INPROGRESS、缺查询键404、非法/奇数hex、非UTF-8字节和单引号错误、部分/反向范围，以及Scatter真实RPC错误后的INPROGRESS。仅使用一个Region避免Go map顺序差异，不做字段删除、排序或其他响应归一化。

PENDING优先级通过当前canonical `task_124_replication_and_rules`直接运行真实RuleManager和CheckerDispatcher验证：耗尽真实placement候选，执行checker产生rule-pending，再检查Handler返回PENDING；改变真实规则后健康fit优先返回REPLICATED。HTTP运行差分覆盖REPLICATED/INPROGRESS及重试队列区别；不把领域PENDING测试描述为真实TCP的PENDING差分。宿主自动checker巡检不是本次HTTP装配的实现范围。

follower测试读取同一真实etcd MetaStore，验证默认转发、显式本地异常查询500及同步后本地range-holes。实际事务阻塞时发送HTTP请求并确认仍在等待，宿主取消后响应500、shutdown完成；读成功和取消前后逐entry比较raft前缀不变。

开发中的失败尝试保留在压缩证据：首次Go流等待响应头前没有发送首条消息，监督入口以130终止并清理；修复发送顺序并添加超时。随后差分发现单引号字节错误文本多了反斜杠，已按Go运行输出修复。自动巡检/Scatter重试队列被误用作rule-pending的两次等待超时，不计成功；修正为明确检查两个队列的区别，PENDING使用canonical领域测试。首次follower等待条件误用cluster-only异常路由，已改用允许本地读取的range-holes观察同步屏障。

[只读监督入口](../../../scripts/http-region-check-oracle.py)复用任务004的Go守卫、进程组清理和精确计数，显式运行默认忽略的差分测试。Go oracle为已有bin/pd-server；构建信息没有VCS revision，不声称对应当前HEAD。Rust外部etcd不代表Go embedded-etcd拓扑等价。

## 验证

实际动态领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。所有Cargo和调用Cargo的脚本均继承该路径；默认features，`CARGO_BUILD_JOBS=2 GOMAXPROCS=2 RUST_TEST_THREADS=4`。修改后先运行cargo fmt --all。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_059_ -- --nocapture` | 0 | 2通过、0失败、1忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-region-check-oracle.py` | 0 | 1通过、0失败、0忽略；79组完整响应/实现 |
| `cargo test --locked -p astersql-pd-schedule-handler --lib task_124_replication_and_rules -- --nocapture` | 0 | 1通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 205通过、0失败、20忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked`（首次） | 101 | etcd fixture重启遇磁盘耗尽，非业务断言；原始日志保留 |
| `cargo test --locked -p astersql-pd-etcdutil --lib snapshot_watch_etcd -- --nocapture` | 0 | 1通过、0失败、0忽略 |
| `cargo test --workspace --locked`（清理临时文件后重跑） | 0 | 2100通过、0失败、57忽略 |
| `git diff --check`、Python AST及本文链接检查 | 0 | 静态检查 |

首次workspace因`no space left on device`中断。只清理本阶段生成、且PID已经退出的每进程etcd fixture临时可执行文件；不删除Cargo编译缓存、其他任务锁或其他用户文件。重跑workspace和最后一次真实差分使用`RUST_TEST_THREADS=2`降低临时磁盘峰值，其余验证使用4；Cargo参数与默认features不变。差分在最终源码再次运行通过；默认忽略项均不计成功。

状态：已完成。开始/结束Go守卫覆盖1101个文件（含ignored/untracked），存在性、逐文件SHA-256及Git状态一致；plan.md逐字节不变。冻结的1266个Rust/Cargo输入在最终验证后逐文件核验一致。所有验证进程及服务已结束；自己的槽位1 owner记录和锁已清理，保留Cargo缓存。

原始红灯、开发中的失败尝试、聚焦/包级/workspace最终日志、确切命令/退出码/测试计数、Go构建信息和二进制SHA-256、Go守卫及Rust输入哈希见[压缩证据](task-059-evidence.json.gz)。SHA-256 `84cdea41391d5f1a44d5f5932eec431f3decaf17985535ebbe3f04cd1eb9659f`。

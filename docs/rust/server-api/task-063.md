# 任务063：Operator管理HTTP

基准 `ab0f3ed42`。已读取只读plan.md、任务63、任务001–004/062证据与三份来源清单，批次62依赖满足。Go源码和Go模块全程只读；协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与实现

来源：source-inventory.md:101、251、511–512及1557–1585的共享handler来源快照；wire-inventory.md:134–135、138（HTTP-002/003/006）；surface-inventory.md:4731–4732、4735。清单中的共享handler来源不代表其所有管理HTTP均归本任务；审计/限流仍由81/82验收。

- [HTTP写适配](../../../pkg/server/http_operator_write.rs)在[既有operator路由](../../../pkg/server/http_operator_read.rs)接入POST `/pd/api/v1/operators`、DELETE `/operators`与DELETE `/operators/{region_id}`；未新建平行服务。
- [PdServices](../../../pkg/server/lib.rs)、[Application](../../../pkg/runtime/node.rs)、[RoleServices](../../../pkg/runtime/member_role_worker.rs)把意图交给[原调度队列](../../../pkg/runtime/schedule_scatter.rs)；[宿主执行](../../../pkg/runtime/schedule_operator_write.rs)校验term，借用同一Coordinator、RuleManager、HotStat、affinity及fenced allocator，调用canonical Handler。新建Operator进入已有schedule_creations/dispatch与心跳重试队列，删除/冲突/历史复用领域所有者。
- [领域解析器](../../../pkg/schedule/handler/admin_parse.rs)保留batch的类型化错误供适配层渲染错误码，维持HTTP-200 scatter响应；没有把HTTP/proto/etcd类型引入领域层。server仅新增已有workspace handler依赖，Cargo.lock只记录该依赖边；无浮动协议或可选feature。
- 真实Go数字差分纠正了此前ID输入边界：float64舍入后截断，小数ID不直接拒绝；负数按uint64转换，固定amd64 Go oracle中越界ID为2^63，retry_limit越界为i64::MIN。同步修正[旧fixture](../schedule-execution/fixtures/task_121.json)的负数、小数和越界三项及[旧测试](../../../pkg/schedule/handler/task_121_test.rs)，保留其余领域验收。[说明](../../../pkg/schedule/handler/admin_parse.rs.md)区分历史边界与当前运行证据。

只读Go源码映射：[operator HTTP](../../../server/api/operator.go)、[router](../../../server/api/router.go)、[Handler](../../../pkg/schedule/handler/handler.go)、[转换](../../../pkg/utils/typeutil/conversion.go)、[scatter](../../../pkg/schedule/scatter/region_scatterer.go)、[错误码](../../../pkg/errs/errno.go)。

## 行为与差分边界

[独立TCP测试](../../../pkg/server/http_operator_write_test.rs)显式注册。通过真实配置HTTP在双方开启halt-scheduling与placement rules，以隔离命令契约，避免自动调度干扰；不把测试显式配置当作默认配置等价证据。真实bootstrap、PutStore、StoreHeartbeat与RegionHeartbeat建立4个store、3个相邻region。

命令表覆盖transfer-leader、transfer-region、transfer-peer、add-peer、add-learner、remove-peer、merge-region、split-region、scatter-region、scatter-regions：逐项缺参、成功及冲突。每个冲突对同一个真实运行sentinel发起，比较完整活动集合和稳定状态，保证其他Operator不变；batch保留200和失败百分比。非随机命令比较整个Operator object（包括brief、epoch、kind、timeout、status），真实心跳RPC核对transfer-leader目标peer。随机scatter按实际region检查活动对象及删除结果，保留响应百分比，不伪造固定Operator。

单删、重复删、全删、重复全删、缺失region/operator、坏ID与uint64溢出、DELETE `/operators/records`落到动态ID校验、peer缺失/已存在、store缺失、非相邻merge、非法split policy/key、placement role缺失/数组类型错误，以及float64小数/负数/2^53舍入/2^64边界/极大指数均有当前证据。单删merge source同步取消merge pair，sentinel保留；取消后读取CANCEL。重启前留有真实RUNNING Operator，重启后两实现均清空内存Operator，保留同一MetaStore中的集群元数据；不声称Operator历史已持久化。

真实MetaStore故障：复用[事务故障门](../../../pkg/server/http_store_state_test.rs)，新增只命中alloc_id事务的模式。先用真实AllocID RPC耗尽缓存窗口，再向真实etcd事务加入无效lease，使分配返回错误；确认未admit、活动集合与持久分配边界不变，然后正常创建恢复。另在真实store持久事务阻塞时排队DELETE，全程确认等待，宿主取消后返回500、shutdown完成，raft元数据不变。follower默认转发、显式本地处理返回未bootstrap错误亦经真实TCP验证。这些故障为Rust边界证据，不冒充Go故障注入差分。

[只读监督入口](../../../scripts/http-operator-write-oracle.py)复用任务004二进制检查、Go哈希守卫、进程组清理及精确有效测试计数。Go oracle为已有bin/pd-server，build info无VCS revision，不声称由当前HEAD构建。其余场景的状态、错误体、对象、重启结果逐项相等；完整原始输出随证据保存。

**明确差异：** Go server的GetCluster返回typed-nil接口，未bootstrap的合法单区创建会触发negroni Recovery，返回含不稳定Go堆栈的text/plain 500。测试保留并打印原始错误体；对冷启动scatter-region和transfer-leader只比较500状态。Rust返回稳定的ErrNotBootstrapped JSON 500，避免panic。该分支错误体/Content-Type不等价，未用归一化声明其完全兼容；其他响应字段未因此删除。keys非数组也保留领域既有400输入保护，不复制Go类型断言panic。

## 验证记录

初始`cargo test --locked -p astersql-pd-server --lib task_063_ -- --nocapture`可编译运行1个真实TCP测试，POST缺JSON写路由导致行为断言失败，退出101。补充数字用例后再次可运行并失败：Go合法负数被旧解析器以400拒绝，退出101；真实Go差分进一步确定溢出哨兵值。开发过程中的bootstrap默认配置不同、错误码映射、分配缓存窗口及旧fixture修正均保留原始失败，不计为通过。

首轮包级回归215通过、1失败、24忽略，失败为既有task_048_behavior重启监听器AddrInUse；随后串行执行，避免不同测试竞争重启端口。第二轮215通过、1失败、24忽略：任务62将第二轮随机scatter固定断言为2个Operator，实测1个。

依赖测试修复：[任务62测试](../../../pkg/server/http_operator_read_test.rs)的取消阶段改由本任务真实POST创建region100/101的两条transfer-leader Operator，并补真实StoreHeartbeat；继续要求两条RUNNING→CANCEL及完整记录，不降低取消数量/状态断言，不修改领域生产状态机。初轮随机scatter的RPC成功/完成/记录验证保留。修改后的任务62真实Go差分另行显式重跑；其历史task-062.md与压缩证据保持原样，不冒充当前源码证据。默认忽略的真实Go差分由监督入口显式运行，其通过数单列。

额外回归修复：[TSO独立集成测试](../../../pkg/tso/etcd_behavior_test.rs)在串行运行时，libtest将测试名前缀与子进程的PROCESS_TSO标记拼到同一行。workspace首轮2094通过、1失败，子进程实际输出了正确时间戳且退出0，父进程因strip_prefix漏读而失败。默认workspace集合过滤该测试复现了实际行为失败（退出101）；让标记显式另起一行，并增加失败时stdout诊断，保留原有两次进程退出/恢复、物理逻辑值及持久窗口严格断言。修改仅限该独立测试目标，没有改动TSO生产代码。尝试单独选择TSO crate时遇到其既有serde feature unification编译缺口，随后按默认workspace集合运行名字过滤，不添加可选feature；编译失败不计行为红灯。

server/handler及差分输入已通过后，只有上述TSO独立测试发生变化。续跑重新格式化、冻结输入，执行metadata/格式检查、server检查、TSO精确过滤、workspace检查与全量测试；两阶段唯一差异与各命令输出均保留，避免为独立测试的输出格式修复重复执行无关单包命令。

最终阶段动态领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。默认features，`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 GOMAXPROCS=2 PD_COMPAT_GO_BIN="$PWD/bin/pd-server"`。以下最终命令均退出0。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 静态检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_063_ -- --nocapture` | 0 | 2通过、0失败、1忽略 |
| `python3 scripts/http-operator-write-oracle.py` | 0 | 1通过、0失败、0忽略 |
| `python3 scripts/http-operator-read-oracle.py` | 0 | 1通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 217通过、0失败、24忽略 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-schedule-handler -- --nocapture` | 0 | 45通过、0失败、0忽略 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2117通过、0失败、61忽略 |
| `cargo fmt --all` | 0 | 静态检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo test --workspace --locked real_etcd_oracle_process_restart -- --nocapture` | 0 | 1通过、0失败、0忽略 |
| `cargo check --workspace --locked` | 0 | 静态检查 |

状态：已完成。最终4423个源码/Cargo/JSON fixture/Python/TOML输入在格式化后冻结，续跑全量命令结束逐文件SHA-256一致。server/handler及两组差分的通过记录保持有效：续跑前后唯一变化是独立TSO集成测试的stdout换行，相关生产与server测试输入均未改变；两阶段完整输入清单及唯一差异另存于原始证据。开始/结束1101个Go/模块文件（含ignored/untracked）的存在性、内容SHA-256与Git状态一致，plan.md原字节不变。未运行make、Go测试、Go格式化、模块整理或failpoint启停。

监督入口确认两个oracle进程组均清理；测试、服务、fixture和worker已退出，自己领取的槽位锁已释放，缓存保留。本文相对链接、Python AST与git diff --check另作提交前核验。原有任务58删除及prompt.md修改不纳入提交。

[压缩原始证据](task-063-evidence.json.gz)，SHA-256 `7f05dc6de5ddb69b5e344da171a1428b83c5cc6ddac750c6695834f98921fc90`。包含红灯/开发失败、两轮包级失败、TSO串行输出失败与修复、最终全部命令输出/有效计数、两组Go build info及原始HTTP/RPC观察、Go守卫与冻结输入清单、owner/PID及清理记录。写接口差分84条观察，其中10条命令冲突；冷启动panic错误体差异见上文，不声称完全相等。

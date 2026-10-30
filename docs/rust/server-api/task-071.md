# 任务071：Affinity查询HTTP

状态：已完成。基准 `c0bca1e05`；依赖[任务070](task-070.md)及[任务004](task-004.md)只读oracle入口已核验。Go与模块文件、plan.md只读；默认features与固定kvproto版本不变。

## 来源与实现

[source-inventory.md](source-inventory.md):109为本任务摘要；owner含71的全部来源行为扫描行是209–225、270、412–494、530–531、1341–1387、2526–2551、2614。共享keypath、labeler、affinity领域实现复用原canonical crate，原盘点哈希仍作为基准；这些共享所有者的写入、调度和其他HTTP行为不计作本任务新增验收。[wire-inventory.md](wire-inventory.md):309–310对应HTTP-177/178，本任务只更新这两个查询入口及来源摘要的覆盖记录。第三份[surface-inventory.md](surface-inventory.md)没有owner=71独立行；既有配置、指标、审计和安全前置仍由原owner负责，不扩大声明。

只读Go oracle为`server/apiv2/handlers/affinity.go`的GetAllAffinityGroups/GetAffinityGroup/handleAffinityError、`pkg/schedule/affinity/{parse,manager,group}.go`以及已有ranges/labeler持久化恢复逻辑。Go实际GroupState没有range明细，只有range_count；查询不引入请求结构的base64编码或重新编码持久化hex边界。

[HTTP适配](../../../pkg/server/http_affinity_read.rs)接入原router/PdServices、active leader与宿主调度命令队列。[宿主适配](../../../pkg/runtime/affinity_read_http.rs)在同一term admission锁下检查bootstrap并读取既有Manager。Manager在原调度任务内从相同MetaStore和已加载的RegionLabeler恢复，由该任务唯一持有，随term退出释放；原调度任务使用现有10秒可用性检查策略，准备阶段和运行阶段都驱动该timer，退出时timer随任务释放，没有新增worker。没有第二套group存储、私有runtime或固定响应。传输适配的GroupStateProvider仅借用manager并调用已有state/collect_group_states/validate_group_id；写入接口留给任务072接入同一manager。

与Go一致，列表未给ids时返回全部，给出全空ids时返回空对象；重复ids去重、trim空白、跳过缺失组，任一非法ID返回400。单组ID不trim，非法400、缺失404；未bootstrap优先返回500，错误为JSON字符串。保留Gin v2的`application/json; charset=utf-8`；编码路径只解码一次且路径加号保持字面值。Gin在路由阶段处理编码斜杠，适配返回相同404/text/plain响应。无序对象按JSON对象语义比较，没有归一化group字段或错误。

## 行为证据

[独立测试](../../../pkg/server/http_affinity_read_test.rs)通过真实TCP HTTP、tonic bootstrap和真实etcd验证。初始测试可编译执行，缺失路由返回application/grpc，Content-Type断言失败；退出101，1失败、0忽略，不以编译失败证明缺陷。

Go差分复用现有bin/pd-server；不重建Go服务器、不执行Go测试/make/failpoint。使用任务004既有只读fixture调用链构建etcd工具。双方用Rust客户端bootstrap，通过真实MetaStore事务写入相同固定持久数据，再重启以运行各自真实manager恢复：两个普通group、一个null group，以及带00FF/6100/FF00混合大小写hex边界的两段标签规则。固定create_timestamp无归一化，null恢复后的零值group也原样比较。HTTP读取不修改这些持久字节，双方重复重启结果一致；Rust follower经既有转发入口返回相同完整观察。

补充检查发现恢复的manager还需要宿主驱动可用性策略。新增真实HTTP测试在同一Tokio宿主时钟上pause/advance/resume小步推进至10秒检查点，先得到pending而非preparing的行为断言失败（退出101、1失败），再接入既有check_groups_availability。一次性推进11秒会干扰真实租约续约时序，重跑未取得有效GroupState（phase读取为null）；这次失败不用于产品行为验收。改为100ms小步推进并让真实HTTP/租约处理继续，保留该失败记录。Rust返回的完整已分配GroupState与现有Go进程第三次恢复后经自身真实10秒检查返回的对象完全相同；二者都逐字段核对相同固定JSON，没有归一化phase或计数。Go使用20秒上限观察真实进程，未注入时钟或重建Go。此前工作区回归为修复此发现而主动SIGINT退出130，保留不完整输出，不计为完整通过；最终全部检查重新运行。

19项观察包括全部/过滤/重复/空白/空ids、未知查询字段、非法URL查询pair/分号、Unicode空白、合法与非法路径ID、缺失组、65字节ID和编码斜杠；同时断言range_count、pending、零值计数和不返回range明细。差分最初发现Gin的charset大小写差异，按实际响应修复后重跑通过。Go二进制构建信息没有VCS revision，不声称它来自当前HEAD；外部etcd不宣称等价Go embedded-etcd拓扑。

真实后端错误测试将一次bootstrap读取的revision设置为未来值，由实际etcd返回错误并映射HTTP500；随后恢复正常读取，HTTP及持久快照不变。取消测试只挂起相同真实IO边界，取消宿主后HTTP500，shutdown完成join且持久数据不变。这两个故障场景不冒充Go failpoint差分；没有用mock调用次数验收。

## 验证记录

实际动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；每阶段原子mkdir领取并记录任务/会话/PID，子Cargo及fixture继承该路径。环境CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=1、GOMAXPROCS=2。所有验证完成；确切命令与有效测试数如下。

| 确切命令 | 退出码 | 有效测试数量 |
| --- | ---: | --- |
| 可用性接线前 `cargo test --locked -p astersql-pd-server --lib task_071_availability_tick -- --nocapture` | 101 | 1失败、0忽略 |
| 基准 `cargo test --locked -p astersql-pd-server --lib task_071_ -- --nocapture` | 101 | 1失败、0忽略 |
| `cargo fmt --all` | 0 | 静态检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_071_ -- --nocapture` | 0 | 4通过、0失败、1忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 241通过、0失败、32忽略 |
| `cargo test --locked -p astersql-pd-schedule-affinity -- --nocapture` | 0 | 44通过、0失败、0忽略 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2142通过、0失败、69忽略 |
| `cargo test --locked -p astersql-pd-server --lib task_071_availability_tick -- --nocapture`（重复5次） | 0 | 每次1通过，累计5通过、0忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-affinity-read-oracle.py` | 0 | 1通过、0失败、0忽略；19项查询观察、已分配状态及双方重启快照 |

server包级241有效测试包括240单元和1集成；默认32忽略项中的本任务Go差分已按上表单独显式运行。零测试doc-test不计入有效测试。全部工作区有效测试汇总以原始输出为准，没有用编译成功替代行为验收。

最终1304项Rust/Cargo输入在整轮验证前后SHA-256相同，且与Go差分的1304项共享源码输入相同。全程1101个Go/模块文件（含ignored/untracked）的存在性、SHA-256及Git状态与开始一致；plan.md与HEAD原字节一致。自己的slot-1占用记录与锁已清理，槽位产物没有存活子进程，缓存保留。

[压缩原始证据](task-071-evidence.json.gz)，SHA-256 `d1ae275a0f7d3683f0249ddffc368b2b629082d80c1790687fa0013647d40696`。包含初始行为失败、开发期输出、最终完整命令/退出码/测试数、Go构建信息与哈希、源码输入/Go保护集合和监督清理结果；gzip/JSON往返已核对。Go二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。Python AST、Markdown相对链接和git diff --check另行检查。

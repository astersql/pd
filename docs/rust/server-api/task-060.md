# 任务060：实时热点查询HTTP

基准 `a7c483751`。任务059、任务001的主题6准入和任务004只读oracle入口已核验；plan.md、Go文件和模块文件保持只读。

## 来源与装配

- [source-inventory](source-inventory.md)任务60行和hot_status.go来源行；[wire-inventory](wire-inventory.md) HTTP-077、078、080、081；[surface-inventory](surface-inventory.md) router.go:246、247、249、250。history属于任务61，审计/限流属于81/82。
- [HTTP适配](../../../pkg/server/http_hotspot.rs)、[独立测试](../../../pkg/server/http_hotspot_test.rs)、[共享读取包装](../../../pkg/server/http_region_scan.rs)、[宿主](../../../pkg/runtime/cluster_host.rs)、[角色读取](../../../pkg/runtime/member_role_worker.rs)、[term队列](../../../pkg/runtime/schedule_scatter.rs)。lib.rs显式注册测试。
- 复用canonical Handler::get_hot_regions/get_hot_stores/get_hot_buckets、Coordinator::get_hot_regions、HotStat、StoresStats和bucket FIFO worker。没有新建统计领域实现、缓存、worker、feature或依赖。

读写热点只向对应方向的现存HotCache排队查询，不依赖另一个方向或bucket队列的可用性；Store速率读取同一HotStat的真实滚动窗口。bucket使用当前配置阈值、真实FIFO查询和canonical大写hex响应。宿主恢复、bootstrap成功及Store注册成功时初始化零值Store窗口，镜像Go schedulingController的启动和updateStoreStatistics行为；查询不写MetaStore。读热点命中阈值按Go schedulingController放大六倍，统一60秒Region窗口与10秒Store报告的热度口径。

重复store_id按请求顺序逐个解析并读取Store，保留“未知Store先于后续格式错误”的500/400优先级；缺失/空值、十进制前导零、负数、加号、溢出、非法转义、非UTF-8、分号和重复参数均由真实HTTP验证。bucket无效region_id变成0，未知Region省略，重复ID不重复统计。没有把无效region_id错误地当作空ID列表查询全部。

Go GetCluster返回typed-nil接口：未bootstrap的stores/buckets/有效store_id查询触发外层恢复器，返回文本500；无store_id的Region查询返回JSON ErrNotBootstrapped。沿用任务049的处理方式，Rust直接返回同一状态、Content-Type及PANIC首行，不触发Rust panic或暴露恢复栈。普通follower仍通过现有包装转发leader，显式本地Store热点查询保留该未bootstrap错误。取消沿现有请求上下文和term队列传播，已关闭的reply不会保留查询任务。

## 行为与差分

初始真实TCP测试已编译运行，`cargo test --locked -p astersql-pd-server --lib task_060_ -- --nocapture`退出101、1测试失败：Content-Type为application/grpc，证明路由缺失；不以编译错误或磁盘失败充当行为红灯。

真实TCP/tonic bootstrap、注册Store、Region/Store心跳和bucket报告，比较41组Go/Rust HTTP观察。覆盖零值窗口、稳定1 MiB/s读写、Store 2 MiB/s写速率、TiFlash compute过滤、as_peer/as_leader和nullable Store过滤、读阈值六倍换算、连续冷报告后的write消失、bucket热度增长/冷却、成员迁移后的旧read peer过期、成功和参数错误、未bootstrap错误。Rust另验证同一MetaStore重启后恢复成员并重建零值窗口，以及HTTP读取前后raft前缀逐entry不变。

同一真实MetaStore上的双节点验证默认follower转发和显式本地错误；真实事务阻塞后发送热点HTTP，确认等待，再取消宿主，要求500、shutdown完成且持久状态不变。TTL边界通过真实HotCache worker的显式准入时钟验证：读写方向分别在默认TTL前1ns保留，在截止相等时删除，后续查询为空。TTL测试是Rust worker证据；不冒充Go时钟推进或HTTP墙钟TTL差分。

[只读监督入口](../../../scripts/http-hotspot-oracle.py)复用任务004守卫、进程组清理和精确测试计数，显式执行默认忽略的差分。Go为已有bin/pd-server，build info没有VCS revision，不声称由当前HEAD构建；固定双方TZ=UTC后逐值比较write报告时间。read到达时间先验证RFC3339，再替换该单一动态字段；仅将2^53内可精确表示的整值浮点数统一JSON数值拼写，不改变整数ID/计数、分数速率、数组顺序、空集合或null项。Go PANIC保留原始恢复输出，只排除恢复栈，比较首行及状态/Content-Type。Rust外部etcd不代表Go embedded-etcd拓扑等价。

## 环境恢复与验证

原会话因磁盘138 MiB、etcd无法创建WAL而阻塞。继续执行时确认无fixture进程及对应PID后，仅清理1116个已退出测试的临时rust-etcd-fixture可执行文件（42,869,899,008 bytes）及一份遗留etcd-contract数据，释放约40 GiB；没有清理Cargo缓存或用户文件。随后外部target删除导致owner锁和rmeta缺失；重新原子领取槽位，尝试归档元数据恢复未通过编译器校验，保留旧缓存指纹后由Cargo完整重建，才恢复可用验证。没有将这些环境失败计为业务红灯或成功。

开发中另修正ReportBucketsRequest的默认字段、Region同步辅助请求的octet-stream类型、JSON整值浮点比较、Go typed-nil错误、读阈值换算、零值窗口，以及重启测试复用持久化成员URL；本轮记录的失败命令输出和差分原始日志保留在压缩证据。

实际动态领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。所有Cargo及调用Cargo的脚本继承该绝对路径，默认features，CARGO_BUILD_JOBS=2、GOMAXPROCS=2；初期RUST_TEST_THREADS=2，最终验证为4。最终源码先运行cargo fmt --all，再执行聚焦及其余验证。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_060_ -- --nocapture` | 0 | 3通过、0失败、1忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-hotspot-oracle.py` | 0 | 1通过、0失败、0忽略；41组观察/实现 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 208通过、0失败、21忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2103通过、0失败、58忽略 |
| `git diff --check`、Python AST及本文链接检查 | 0 | 静态检查 |

状态：已完成。最终源码1195个Rust/Cargo输入逐文件SHA-256一致；开始/结束1101个Go/模块文件（含ignored/untracked）的存在性、内容SHA-256和Git状态一致；plan.md逐字节不变。默认忽略项不计成功，任务060差分已通过监督入口显式执行。所有本阶段服务、测试及worker已退出；清理39项本阶段已结束fixture临时产物，槽位1 owner和锁已释放，保留Cargo缓存。未提交任务58删除或prompt.md的用户已有修改。

[压缩原始证据](task-060-evidence.json.gz)包含命令、退出码、有效计数、失败/通过日志、全部Go差分尝试及原始恢复栈、二进制构建信息/SHA-256、守卫、来源行、最终源码哈希及槽位清理记录。SHA-256 `3b0ea9c42760bbe410b453aa20484ff4a7f5fb64cf6a8b744844376d0ce6f45c`。

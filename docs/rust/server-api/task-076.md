# 任务076：TSO恢复管理HTTP

状态：已完成，待回归。实现、聚焦、Go差分、server包级及workspace check已通过；完整workspace存在无关fixture失败，待批次统一回归。

基准 `3ab5a2ae7`；依赖[任务075](task-075.md)已核验，复用[任务004](task-004.md)只读Go入口。协议固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`；无依赖、feature或Go变更。

## 来源与边界

[source-inventory.md](source-inventory.md)中owner含76的91行已逐行核查：192–201、237、267、346–399、497–499、2493–2512、2574、2610。本任务覆盖192/198/200中resetUserTimestamp相关行为与其持久窗口接线；193/194/196及其共享worker/配置来源复用既有35/36能力，不宣称独立MCS/keyspace-group服务、完整指标或failpoint逐项覆盖。237/2574为admin.go共享文件来源，缓存和标记/ID恢复仍由73–75验收；497–499同属73缓存管理。267/2610及[wire-inventory.md](wire-inventory.md)的HTTP-168 local TSO转移明确交主题8：当前Go入口已经deprecated，本任务按要求只接reset-ts，不以固定成功响应模拟转移。HTTP-169为本次接线；[surface-inventory.md](surface-inventory.md)没有独立owner=76行，审计与限流继续交81/82。

Go oracle为[pkg/tso/admin.go](../../../pkg/tso/admin.go)的ResetTS、[pkg/tso/tso.go](../../../pkg/tso/tso.go)的resetUserTimestamp及[server/config/config.go](../../../server/config/config.go)的默认24小时max-gap-reset-ts。输入tso必须为非空十进制uint64字符串；force-use-larger必须为bool。普通恢复拒绝物理倒退、同物理逻辑小于等于当前值、物理差值大于等于上限；force同时忽略旧值和绕过gap检查。新窗口先持久成功再发布内存状态。

[适配器](../../../pkg/server/http_admin_recover_tso.rs)复用JSON/整数解析、请求取消、leader转发，并保留TSO独立Go renderer的JSON末尾换行。通过[PdServices](../../../pkg/server/lib.rs)、[RoleServices](../../../pkg/runtime/member_role_worker.rs)和[Application](../../../pkg/runtime/node.rs)操作同一活动term的[Oracle](../../../pkg/tso/oracle.rs)，上限实时读取现有持久配置。领域reset与allocate/update共用window锁、leadership fencing和真实MetaStore事务；失败或取消不发布新内存窗口。没有在HTTP层复制领域算法。

保留既有Oracle的可持久物理范围检查及窗口fencing；本次证明的force行为覆盖超过配置上限的正常可表示时间，不把超出int64 UnixNano持久范围的任意远未来值宣称为兼容通过。Rust外部etcd与Go embedded-etcd拓扑不同，不宣称原生后端或崩溃恢复等价；底层etcd错误及取消为Rust真实IO故障验证，不声称Go failpoint差分。

## 失败与行为证据

[独立测试](../../../pkg/server/http_admin_recover_tso_test.rs)全部task_076_前缀并在lib.rs注册。初始测试已编译运行，真实HTTP返回`(200, "", "application/grpc")`，参数错误预期400，退出101，0通过/1失败。未以编译错误充当行为失败。

首次Go差分退出1（内部Cargo退出101，0通过/1失败）：状态码、字符串和实际分配副作用一致，但Go JSON正文末尾有换行，Rust无换行。修复本端点renderer后，保留原始正文逐字节比较，仅Content-Type转小写。

真实TCP HTTP/tonic/etcd覆盖缺字段、null、数字/空/带符号/溢出tso、非bool force、物理倒退403、普通前进200、gap上限403、force跳过gap及force忽略旧值；核对后续GetMinTS实际大于目标值。真实第二节点转发成功，leader RPC观察新值；重启后越过持久纳秒窗口。

手动时钟host持有真实Oracle和真实etcd，通过同一HTTP/RPC入口验证同物理相等逻辑拒绝、上限恰好相等拒绝、上限减1成功、force较小/相等不推进边界、时钟回拨后继续单调。事务故障在真实写事务附加不存在lease的写入，使etcd拒绝整笔事务；HTTP403、持久边界与内存分配均保持，随后正常恢复成功。取消在真实持久事务IO之前暂停，取消HTTP请求/Oracle，HTTP500、持久边界不变，服务及worker退出。

[只读supervisor](../../../scripts/http-admin-recover-tso-oracle.py)复用任务4守卫，运行既有Go二进制，禁止Go源码、模块、生成或failpoint写入。12组HTTP状态/原始正文/Content-Type一致，并分别验证RPC副作用与第二节点转发；Go重启后继续单调分配。二进制缺VCS revision，不声称由当前HEAD构建。

## 验证记录

实际动态领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。所有Cargo、supervisor及子进程继承该绝对路径，默认features，`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 GOMAXPROCS=2`。每阶段锁目录记录plan/task/session/PID，未使用--target-dir。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 静态检查/格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_076_ -- --nocapture` | 0 | 2通过、0失败、1忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查/格式化 |
| `cargo fmt --all -- --check` | 0 | 静态检查/格式化 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查/格式化 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 255通过、0失败、37忽略 |
| `cargo check --workspace --locked` | 0 | 静态检查/格式化 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-admin-recover-tso-oracle.py` | 0 | 1通过、0失败、0忽略 |
| `cargo test --workspace --locked` | 101 | 1960通过、1失败、74忽略 |
| `cargo test --locked -p astersql-pd-storage-endpoint --lib meta_service_group_test::meta_service_group_etcd -- --exact --nocapture` | 0 | 1通过、0失败、0忽略 |

首轮workspace在未改动的`meta_service_group_test::meta_service_group_etcd`失败：等待fixture确认文件时etcd子进程退出，断言位于`pkg/storage/kv/etcd_fixture_test.rs:77`；该crate为102通过/1失败。日志没有给出确切退出原因，当时可用空间约408MiB，不将磁盘压力推断冒充已定位根因。本任务生产源码及聚焦测试没有因此变更。

确认Cargo、测试及槽位产物进程停止后，进行缓存透明压缩：77组对象回收477,515,776字节，340组库文件回收3,070,263,296字节。每组先核对全部硬链接均在本槽位，校验原始SHA-256和长度，再替换压缩副本并重连、核对inode一致；缓存字节保留，记录随证据保存。随后释放旧锁、重新原子领取槽位1，仍使用同一绝对CARGO_TARGET_DIR。原失败测试单独重跑通过。

第二轮完整workspace以`RUST_TEST_THREADS=2`开始；用户随后明确要求不让无关全量回归重复阻塞实现批次，故向本次Cargo及其测试子进程发送SIGINT并清理确认，命令退出-2。该中断不计为通过，也不伪装成新的实现失败；完整日志与进程清理记录保留，统一回归将在后续批次恢复。其余最终命令使用`RUST_TEST_THREADS=1`。

最终1101项Go/模块文件（含ignored/untracked）存在性、SHA-256及Git状态与开始一致；1315项Rust/Cargo输入在完整回归前后SHA-256一致，plan.md字节不变。Python AST、Markdown链接、gzip/JSON往返及git diff --check通过。

本任务33个fixture临时二进制仅在核实持有PID已退出且lsof无引用后清理，共1,267,658,304字节；未删除之前已有的fixture。全部本任务服务、worker、Go差分进程组与槽位产物进程已结束，自己的slot-1锁已释放，缓存保留。编号任务文件按用户最新收尾要求标记“已完成，待回归”并保留。原有prompt.md变更不纳入提交。

Go二进制SHA-256为`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，build info为go1.26.8且缺VCS revision。

[压缩原始证据](task-076-evidence.json.gz)，SHA-256 `9c2c2b547d51097e12b79dbb0ef0fdd0ab5e90e972ffb616a7ca19b22b3169a1`。包含预期行为失败、renderer差分失败与修复通过、最终命令/输出/退出码/有效测试数、首轮全量失败、单项重跑、按用户指令中断的重复全量、Go/Rust保护集合、来源映射、缓存压缩及进程/锁/临时fixture清理证据。

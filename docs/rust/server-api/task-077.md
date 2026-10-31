# 任务077：ResolvedTS查询HTTP

状态：已完成。

基准 `264d3d0149c119b247b7575c0981bed32ba2236e`；依赖[任务076](task-076.md)的实现、server包级及workspace check证据已核验，其无关workspace fixture回归不作为本端点的缺失领域能力。[任务004](task-004.md)提供只读Go入口。kvproto保持固定commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`；没有依赖或feature变更。

## 来源和边界

[source-inventory.md](source-inventory.md)中owner包含77的150行已逐行读取并保存原文：250、282–294、551–652、2591、2627–2659；任务映射行115同步更新。[wire-inventory.md](wire-inventory.md)的HTTP-163/164为本次两个GET入口。[surface-inventory.md](surface-inventory.md)没有独立owner=77行。

250/2591的`min_resolved_ts.go`和283的集群getter为本次查询行为；561及623–626的ResolvedTS worker、内存与持久路径复用任务34，本次检查HTTP读到同一上报/持久状态；其余共享集群、心跳、worker、存储、failpoint及manifest行是上下文，沿用6/10/12–16/20/27/34等已有能力。只覆盖ResolvedTS查询所需的初始化、store可用性、上报与持久worker状态，不把整个`server/cluster/`或共享failpoint集合宣称为本次重新验收。主题8/9/10边界保持。

Go源码为[HTTP handler](../../../server/api/min_resolved_ts.go)、[集群getter](../../../server/cluster/cluster.go)和[store可用性](../../../pkg/core/store.go)。无scope返回worker持有的cluster值及null stores；`scope=cluster`包含全部store而保留cluster值；ID列表返回指定store的最小值，去重后的缺失store仍为`u64::MAX`。未初始化、已移除、没有leader或非TiKV store不能提供有效值。store入口也保留null stores。非零间隔包含`is_real_time:true`；零间隔省略该字段，但仍保留`persist_interval:"0s"`。十进制解析保留Go错误字符串。

[HTTP适配器](../../../pkg/server/http_resolved_ts.rs)复用JSON、参数、请求取消及既有leader转发；[PdServices](../../../pkg/server/lib.rs)、[RoleServices](../../../pkg/runtime/member_role_worker.rs)、[Application](../../../pkg/runtime/node.rs)只连接同一[ClusterHost](../../../pkg/runtime/cluster_host.rs)的真实store cache和worker原子值，不复制上报、最小值推进或持久化算法。初始化条件和现有cluster状态查询/worker一致。间隔读取活动配置，端点本身没有持久写入。

## 行为证据

[独立测试](../../../pkg/server/http_resolved_ts_test.rs)在lib.rs显式注册。先保留新接口而不挂路由，测试成功编译并在真实TCP/etcd上失败：未bootstrap入口实际200、预期500；已bootstrap查询实际缺少min字段、预期`u64::MAX`。退出101，0通过/2失败/1忽略，未以编译失败代替行为失败。

接线后的首轮暴露两个测试假设错误：更新配置使用了未注册的`/config/pd-server`，以及取消响应由既有外层redirect middleware返回。改用真实`/config`并断言既有server-not-started取消错误后通过；没有为测试改变公共取消策略。

真实HTTP/tonic/etcd覆盖连续上报42/7/0/MAX/100、cluster持久值100、指定store实时值、重复及缺失ID、removed/无leader store、空/cluster/列表scope、重复query首值、正负符号、溢出及无效参数。零间隔后再上报7，store值变7、cluster和etcd持久值仍100。真实第二节点通过leader转发查询同一值。

单region单voter的未初始化cluster及store均返回MAX。故障在真实etcd读取边界将revision设为MAX，使etcd返回future revision错误，HTTP500；取消在真实元数据读取前暂停，节点关闭取消请求，HTTP500、持久值仍0，worker和服务完成退出。该注入只控制真实IO边界，不以mock调用次数替代结果。

[Go只读supervisor](../../../scripts/http-resolved-ts-oracle.py)运行已有Go二进制和同一Rust场景，24组状态/JSON结果完全一致，另对removed/无leader store执行真实查询断言。仅JSON对象顺序不作为差异，不归一化错误、时间戳、间隔或store值。取消及etcd IO故障是Rust真实故障验证，不冒充Go failpoint差分。Go二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8且无VCS revision，不能声称由当前HEAD构建。

## 验证记录

| 确切命令 / 阶段 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo test --locked -p astersql-pd-server --lib task_077_ -- --nocapture`（预期路由失败） | 101 | 0通过、2失败、1忽略 |
| 同上（最终聚焦） | 0 | 2通过、0失败、1忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-resolved-ts-oracle.py` | 0 | 1通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 257通过、0失败、38忽略 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2158通过、0失败、75忽略 |

修改Rust后已先执行`cargo fmt --all`；所有通过命令使用最终源码。完整workspace在本轮一次跑完，包含上一批失败的`meta_service_group_test::meta_service_group_etcd`，本轮通过；不把该历史失败作为当前阻塞。默认忽略的差分测试不算行为通过，任务77的差分入口另由supervisor显式执行，1通过、0忽略。

首次动态领取槽位1，绝对路径`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；依赖构建反复缺失rmeta，均未运行行为测试。尝试从现有rlib的Mach-O `.rmeta`段恢复缺失元数据；保留缓存，未修改源码或工具链。仍出现其他缺失产物后结束该阶段、释放自己的slot-1锁。在slot-1仍被本会话持有时按顺序尝试下一空槽，原子mkdir领取槽位2，然后释放slot-1；未删除他人的锁或进程。

最终`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`，默认features，`CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=1 GOMAXPROCS=2`。所有Cargo、supervisor及槽位产物进程继承该绝对路径，无--target-dir覆盖。

最终1101项Go/模块文件（含ignored/untracked）的存在性、SHA-256及Git状态与开始完全一致；8项本任务Rust/脚本输入哈希在全部验证后不变；只读plan.md字节与基准一致。Python AST、证据Markdown链接、gzip/JSON往返及git diff --check通过。

自己的Go差分进程组、Rust服务/worker、槽位产物进程和fixture目录均已退出/清理。29个本任务临时fixture二进制仅在确认持有PID不存在且lsof无引用后删除，合计1,114,002,752字节；编译缓存保留。slot-1早期锁及slot-2最终锁均已释放；占位进程收到SIGTERM后已回收。编号任务文件按要求标记完成后删除，原有prompt.md变更不纳入提交。

[压缩原始证据](task-077-evidence.json.gz)，SHA-256 `aea775ad2ed8903f368a4d5e42fafe3e88cc2ca1b8146ebca541fcd1017ebe72`。包含依赖缓存失败、预期行为失败、首次通过轮次的测试假设修正、最终聚焦/差分/包级/workspace完整输出、确切命令/退出码/有效测试数、来源150行原文、文件守卫、实际槽位与进程/锁/临时fixture清理记录。

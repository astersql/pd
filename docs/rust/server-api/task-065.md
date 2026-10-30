# 任务065：注册既有调度器配置路由

基准 `dbb6287e9`，依赖[任务064](task-064.md)及任务001–004的清单和只读入口。状态与最终验证见末尾。plan.md始终只读；不修改Go、模块文件或协议。kvproto仍固定 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与实现

来源：[source-inventory.md](source-inventory.md):170–171、260、521、2089/2092/2602；[wire-inventory.md](wire-inventory.md):350–380（HTTP-218至HTTP-248全部31行）；[surface-inventory.md](surface-inventory.md):4743–4744为共享审计来源，审计仍归任务81，本文不扩大验收范围。

[HTTP挂载](../../../pkg/server/http_scheduler_config.rs)及[独立测试](../../../pkg/server/http_scheduler_config_test.rs)在[lib.rs](../../../pkg/server/lib.rs)显式注册。[主router](../../../pkg/server/http.rs)、[Application](../../../pkg/runtime/node.rs)、[RoleServices](../../../pkg/runtime/member_role_worker.rs)通过[原队列](../../../pkg/runtime/schedule_scatter.rs)把原始method、body、相对路径及query交给同一term coordinator。[宿主分发](../../../pkg/runtime/schedule_scheduler.rs)穷尽匹配canonical BuiltinScheduler，复用已有局部HTTP适配器；响应的状态、Content-Type和body保留，不强制重新包成JSON。没有复制字段校验、配置持久化或调度算法，没有新建服务、worker或MetaStore。

| 策略 | 复用局部适配器及当前验证 |
| --- | --- |
| balance-leader | schedule_admin_balance_leader；list、batch更新/空更新/坏JSON、持久恢复 |
| balance-region | schedule_admin_balance_region；Go继承处理器返回`not implements`，保留其method/body行为 |
| balance-witness | schedule_admin_balance_witness；list、batch更新及重启 |
| evict-leader | schedule_admin_evict_leader；多store更新、坏ID/缺失ID/部分删除/最后删除 |
| evict-slow-store | schedule_admin_evict_slow_store；disk/network共用原实例、batch/recovery-duration及重启 |
| evict-stopping-store | schedule_admin_evict_stopping_store；batch更新/错误及重启 |
| grant-hot-region | schedule_admin_grant_hot_region；leader/store更新和重启；遵守与balance-hot互斥 |
| balance-hot-region | schedule_admin_hot_region；有效配置、字节阈值更新、首次真实调度后的能力降级 |
| grant-leader | schedule_admin_grant_leader；store更新、坏ID/缺失ID/部分删除/最后删除 |
| label | schedule_admin_label；保留Go继承处理器 |
| random-merge | schedule_admin_random_merge；保留Go继承处理器 |
| scatter-range | schedule_admin_scatter_range；按实际实例名挂载、key更新及重启 |
| shuffle-hot-region | schedule_admin_shuffle_hot_region；limit更新、事务失败保留旧值、重启 |
| shuffle-leader | schedule_admin_shuffle_leader；保留Go继承处理器 |
| shuffle-region | schedule_admin_shuffle_region；list/config/roles及角色更新、重启 |
| transfer-witness-leader | schedule_admin_transfer_witness_leader；保留Go继承处理器 |
| evict-slow-trend | schedule_admin_evict_slow_trend；recovery-duration更新及重启 |
| balance-range | schedule_admin_balance_range；list、拒绝config、PUT job、DELETE job及持久队列终态 |

只读Go映射：[外层](../../../server/api/scheduler.go)的handleSchedulerConfig、[Handler](../../../server/handler.go)的GetSchedulerConfigHandler及各局部适配器对应的`pkg/schedule/schedulers/*.go`。实例必须实际存在；未bootstrap为406，缺失实例为普通404，已有实例缺少尾斜杠返回307及HTML，OPTIONS沿用Go公共预检200。局部处理器继续决定405、参数错误及响应编码。

最后store删除沿用领域API保存空配置和回滚策略。其同步remove回调先触发领域恢复原内存，再由宿主在同一次独占访问中执行异步Controller.remove；无tick可插入。失败时保留原策略和store控制，成功后清理策略配置并返回Go原文。真实删除事务故障验证了保留、错误及重试，不把两个持久键宣称为原子事务。

热点配置使用原HotRegionConfig更新API，再把参数交回同一策略。Go在第一次实际调度时检查query/CPU支持，而不是配置GET时检查。新增[HotScheduler能力观察](../../../pkg/schedule/schedulers/hot_schedule.rs)及[worker接线](../../../pkg/runtime/schedule_worker.rs)，复用[已有版本比较](../../../pkg/runtime/cluster_split.rs)；初始能力未观察，真实调度按5.2.0、8.5.7/9.0.0-beta.1门槛更新。8.5.0真实网络场景在首次调度前为byte/key，首次调度后为query/byte；参数更新保留已观察能力，调度使用相同有效参数。

## 行为、失败与差分

初始聚焦测试可编译运行，请求误落入gRPC fallback，断言观察到`application/grpc`而非管理HTTP响应，1项失败。后续开发编译错误单独保留，不算行为红灯。初轮全18类Rust观察通过；Go运行又取得热点有效优先级不匹配及缺尾斜杠301/307不匹配的真实断言失败，修复后重新差分。新增OPTIONS场景还发现Go公共预检返回200，已保留该行为。

当前只读Go差分逐项相等：160条HTTP观察，加9类持久配置重启快照。比较状态、解析后的JSON/原始文本及Content-Type；仅统一JSON数字100与100.0的数值表示。job绝对时间不比较，检查真实创建、坏ID、取消后的job-id/alias/status/ranges终态；不把随机时钟当作固定fixture。

初轮直接比较非终态job快照出现Go running、Rust pending：Go现有AllowSchedule在halt判断前执行checkJob，Rust原worker在halt之后。该既有调度准入顺序不由配置路由负责；本任务未修改它，也不声称halt期间非终态推进时序等价。最终路由差分用真实DELETE将两个job置于稳定取消终态再比较，原始初轮差异保留在压缩证据中。真实调度恢复与热点能力检查另有独立观察，不以固定响应代替领域状态。

真实fixture etcd上的故障由既有StateStore扩展两种scheduler_config键模式：追加无效lease使原事务失败，或在原事务边界挂起。balance-leader保存失败仍200且内存更新、持久键未变；shuffle-hot保存失败500且原limit保留；evict最后store删除的最终配置保存失败500且实例/内存配置保留；此前独立提交的空scheduler配置不回滚，正常重试清理实例。follower请求经原中间件转发观察leader更新。挂起配置事务时请求等待，取消宿主后收到500/server not started、worker退出、持久配置未提交。这些是Rust真实事务/取消证据，不冒充Go故障注入差分。

[只读监督入口](../../../scripts/http-scheduler-config-oracle.py)复用任务004的二进制校验、Go文件SHA-256/Git状态守卫及进程组清理；显式运行默认ignored的差分项，要求恰好1通过、0失败、0忽略。已有bin/pd-server的build info无VCS revision，不能声称由当前HEAD构建；原始build info及二进制哈希保存在压缩证据。外部etcd不代表Go embedded-etcd拓扑等价。

## 最终验证

状态：已完成。默认features；`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1`；Go监督命令另设`GOMAXPROCS=2 PD_COMPAT_GO_BIN="$PWD/bin/pd-server"`。所有Cargo操作在动态原子mkdir领取的槽位1内完成，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。Rust修改后先执行`cargo fmt --all`，最终命令如下：

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo test --locked -p astersql-pd-server --lib task_065_ -- --nocapture` | 0 | 2通过、0失败、1忽略 |
| `python3 scripts/http-scheduler-config-oracle.py` | 0 | 1通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 221通过、0失败、26忽略 |
| `cargo test --workspace --locked` | 0 | 2121通过、0失败、63忽略 |
| `python3 scripts/http-scheduler-oracle.py` | 0 | 1通过、0失败、0忽略 |

默认ignored的任务65差分项由监督入口显式执行，不能把ignored计为行为通过。开发中的Go差分行为红灯保留监督退出1及Cargo子进程101。初始主路由红灯保留完整1失败输出，最初shell未另存退出状态侧车，未伪造其状态记录。

相邻任务64差分首轮退出1/Cargo101，102条观察只有defaults一条不同：Go启动中间态3种，Rust已注册5种；后续观察和重启结果均一致。原测试只等列表非空即采样。未修改相邻测试或生产代码，第二轮102条观察及重启全部相等，退出0；两轮原始记录均保留，不掩盖首轮失败。

最终1101个Go/模块文件（包含ignored/untracked）的路径、逐文件SHA-256及Git状态与开始完全相同；plan.md原字节不变。最终验证前冻结的1288个Rust/Cargo输入在包级、workspace及相邻差分结束后逐项哈希不变。没有执行make、Go测试/格式化/生成、模块整理或failpoint启停。Python AST、本文与侧车相对链接、`git diff --check`通过。

真实服务、fixture、worker及oracle进程组全部结束，槽位产物无存活进程；自己的持锁进程退出、锁已释放，缓存保留。任务58原删除和prompt.md原修改不纳入本次提交。

[压缩原始证据](task-065-evidence.json.gz)，SHA-256 `2a95d77578c851a589990c4091f605707a52e2d64b0c3a332292c086141becaa`。包含开发失败、当前160条Go/Rust观察及9类重启快照、事务故障/取消、相邻两轮差分、完整命令输出与计数、Go二进制build info/哈希、前后Go及Rust/Cargo守卫、槽位占用及清理记录。

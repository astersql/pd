# 任务064：调度器生命周期HTTP

基准 `f6a010dd5`；依赖批次63的[证据](task-063.md)及任务001–004已核验。状态与最终命令见本文末尾。只读plan.md及Go/模块约束贯穿执行；未修改协议、Cargo依赖或Go代码。

## 来源与接线

来源清单：[source-inventory.md](source-inventory.md):102、260、521及1494/2602来源快照；[wire-inventory.md](wire-inventory.md):141–144（HTTP-009至HTTP-012）。[surface-inventory.md](surface-inventory.md):4738–4741的共享审计来源仍归任务81，本任务只验收生命周期和原有公共中间件路径。

[HTTP适配器](../../../pkg/server/http_scheduler.rs)接入GET/POST `/pd/api/v1/schedulers`及POST/DELETE `/schedulers/{name}`；[独立测试](../../../pkg/server/http_scheduler_test.rs)在lib.rs显式注册。[PdServices](../../../pkg/server/lib.rs)、[Application](../../../pkg/runtime/node.rs)、[RoleServices](../../../pkg/runtime/member_role_worker.rs)通过[原队列](../../../pkg/runtime/schedule_scatter.rs)把意图送入同一个term Coordinator。[宿主适配](../../../pkg/runtime/schedule_scheduler.rs)复用canonical Registry、SchedulerController、策略更新及balance-range job队列；原worker继续查询该Controller的pause并运行这些实例。没有复制领域调度实现或创建并行服务，领域未引入proto、HTTP或etcd类型。

只读Go映射：[HTTP](../../../server/api/scheduler.go)、[server Handler](../../../server/handler.go)、[管理查询](../../../pkg/schedule/handler/handler.go)、[Controller](../../../pkg/schedule/schedulers/scheduler_controller.go)、[持久配置](../../../server/config/persist_options.go)。协议仍为kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

默认调度器删除后重添暴露了[领域Controller](../../../pkg/schedule/schedulers/scheduler_control.rs)的空参数边界：启动配置的args=null不能与Rust空切片匹配，追加的第二条启用配置被旧禁用条目遮蔽。真实HTTP删除/重添/重启断言取得失败，再让空切片匹配既有null配置，保留原行并重新启用；没有重写配置子系统。

## 当前行为证据

102条Go/Rust观察逐项相等，数组只排序以消除Go map顺序。场景包括冷启动参数校验与未bootstrap错误、未知scheduler、evict参数冲突/空数组/小数、重复创建、缺失scheduler、空pause请求、null/零/负delay、enabled/paused列表、timestamp参数按存在性生效、暂停时间戳可解析且相差3600秒。

真实创建/查询/暂停/恢复/删除覆盖label、slow-trend、balance-range、shuffle-leader、shuffle-region、random-merge、balance-witness、transfer-witness、shuffle-hot、evict-leader（多store去重）、grant-leader、scatter-range；同时观察默认五种策略。balance-range重复POST调用原job队列并返回重复job错误。evict/grant重复创建更新原实例，忽略创建体中无关batch字段；store后缀删除包括坏ID、缺失store、部分删除和最后store删除，保留Go外层响应null。

默认balance-leader删除后重新添加；重启双方均恢复六个scheduler（五默认及evict），持久配置和实际调度一致。暂停TTL只在term内存中，重启清空，与Go一致；没有声称暂停持久化。重启后先暂停全部策略，再解除halt-scheduling，确认没有transfer-leader；只恢复evict后真实RegionHeartbeat流收到region100从store1到store2/3的命令。目标由随机可行集合验证，不强行固定随机选择；验证原始region、接收peer及目标peer。网络/MetaStore均为真实TCP、tonic、HTTP和fixture etcd；没有mock调用次数断言，也未推进模拟时间。

独立真实事务故障由既有StateStore只在原MetaStore事务追加无效lease触发：最终配置保存失败时返回400、配置键未提交，但已注册策略及其独立配置仍保留；删除配置失败返回500、原实例保留；正常重试删除同时清理持久策略配置。此保留行为沿用Go流程，不错误地宣称跨两个键的原子事务。另在真实store事务阻塞时排队scheduler创建，证明请求等待；宿主取消后两个请求均返回500、worker退出、raft元数据不变。follower默认转发与显式本地处理的未bootstrap错误经真实HTTP验证。事务故障与取消为Rust边界证据，不冒充Go故障注入差分。

[只读差分监督入口](../../../scripts/http-scheduler-oracle.py)复用任务004二进制校验、Go文件哈希与Git状态守卫、进程组清理和精确测试计数。已有Go二进制bin/pd-server的build info没有VCS revision，不能声称由当前HEAD构建。完整build info、二进制SHA-256及原始响应保存在压缩证据。调度器局部配置路由挂载留给任务65，本任务没有将该组路由宣称为通过；external etcd仍非Go embedded-etcd拓扑等价证据。

## 失败验证与开发修正

初始聚焦命令可编译运行1项真实TCP测试，请求命中gRPC fallback而非管理HTTP，Content-Type行为断言失败，退出101。默认删除/重添后的重启检查另取行为红灯：balance-leader未恢复，退出101；空参数匹配修复后通过。开发中的HttpError显示转换编译失败单独保留，不计行为红灯。

Go差分开发时，暂停阶段收到的仅header心跳不能算调度命令；现只匹配真实transfer-leader。另一轮在halt时直接等待Go gRPC响应头会等到其输出，而配置/恢复步骤尚未执行，最终命令等待超时。改为与现有调度测试一致的独立真实流读取任务，在流建立过程中执行管理HTTP；保留暂停无命令及恢复后真实命令的严格断言。取消测试初轮用了要求JSON Content-Type的通用helper，而宿主取消公共中间件返回text/plain；该辅助store请求改为核对原始HTTP状态，scheduler请求仍检查500及server not started，不改生产取消语义。

包级首轮217通过、1失败、25忽略：既有`task_021_behavior`预期连续AllocID `(6,0)`，实得`(134,0)`。后台调度worker使用同一分配器预留128个ID；测试在准备门经过后仍假设没有其他分配者。仅在[该测试](../../../pkg/server/grpc_alloc_id_test.rs)启动前持久化halt-scheduling，隔离被测分配序列，保留精确ID、并发唯一、持久窗口、重启和耗尽断言；没有修改分配器或调度生产代码。该精确过滤测试修复后1通过。任务64生产/测试及Go差分输入未因该隔离发生变化；两阶段冻结清单中唯一变化为此测试文件，最终包级和workspace再次回归。

## 最终验证

状态：已完成。所有下列最终命令退出0；默认features，`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 GOMAXPROCS=2 PD_COMPAT_GO_BIN="$PWD/bin/pd-server"`。每阶段从1–10原子mkdir动态领取，实际均为槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_064_ -- --nocapture` | 0 | 2通过、0失败、1忽略 |
| `python3 scripts/http-scheduler-oracle.py` | 0 | 1通过、0失败、0忽略 |
| `cargo fmt --all` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_021_behavior -- --nocapture` | 0 | 1通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 219通过、0失败、25忽略 |
| `cargo test --workspace --locked` | 0 | 2119通过、0失败、62忽略 |

Go差分默认忽略项由监督入口显式执行，1通过、0失败、0忽略；没有把ignored或零测试计为行为通过。包级首轮的失败、后续隔离修复与最终成功均保留；不能把初轮217通过当作完整成功。

最终1101个Go/模块文件（含ignored/untracked）的路径、内容SHA-256及Git状态与开始完全相同，plan.md原字节不变。两阶段1284个Rust/Cargo输入仅独立AllocID测试发生隔离修改；最终清单在全量验证结束逐项SHA-256不变，任务64聚焦与差分输入从通过阶段起未变。未运行make、Go测试、Go格式化、模块整理、failpoint启停；没有Go文件或模块差异。

真实服务、fixture、heartbeat reader、worker及oracle进程组均已结束；ps核对槽位产物与Go oracle无存活进程，自己的锁已释放，缓存保留。Python AST、本文及侧车说明相对链接、git diff --check通过。原有任务58删除及prompt.md修改不纳入提交。

[压缩原始证据](task-064-evidence.json.gz)，SHA-256 `5d0afaabc7576d259b79c97ea52818fa19ae37f12fdef26abaacc3e6e22f8412`。包含预期红灯、开发失败、事务故障/取消和follower测试、102条真实Go/Rust观察、重启命令、全部命令原始输出及测试计数、Go build info/二进制哈希、前后Go守卫、两阶段输入清单、槽位记录及清理检查。

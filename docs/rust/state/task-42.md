# 任务42：移除节点的迟到心跳

实施前依赖：task-41.md 已完成，提交 ae7aea5d1；累计前置按其记录验收。
已读根规则、Copilot、ADR、移植指南、依赖基线与两份只读plan；无更深AGENTS。
Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0，当前cluster.go/cluster_test.go无差异。
起点1101项Go/模块聚合SHA256为ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4。

## 实施前 oracle

复用任务41的现有ARM64/Linux测试二进制，SHA256
8781f08225e2b8b746217d79fe78e3ff072369cc8ae8941cc8d616a8a77986d9。
已审查TestMain、newTestScheduleConfig、newTestRaftCluster、InitCluster和内存storage调用链；
选中测试无failpoint依赖，无源码/模块/生成写入；测试运行于只读挂载的既有二进制。
运行命令经 [slot.sh](task42/slot.sh) 动态领取槽位2：
`docker run --rm -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/cluster-oracle41.test -test.run '^Test(StoreMutationsRejectTombstoneStore|FilterUnhealthyStore)$' -test.v`。
[oracle.out](task42/oracle.out)：2通过，退出0，先于本任务行为实现。
前者注册Tombstone store1，空stats心跳成功，SetMinResolvedTS失败；后者验证Removed不重写stats并清除rolling。
时间为宿主Unix纳秒，interval为秒，错误沿用store N not found；Removed成功而非统一拒绝。

源码审查与运行证据分别记录：Go HandleStoreHeartbeat先检查Removed，随后SaveStoreMeta、PutStore、Observe。
当前Go源码明确承认并发完整移除之后仍可能重新持久化的残余窗口；并不存在计划所述的第二次完整心跳检查。
因此不把此窗口的Go行为伪称为已保证不持久化。Rust状态领域不拥有持久化入口，也不新增存储回调；
本任务强化真实StoresInfo锁下的提交校验和hot admission，禁止把迟到快照重新put回注册表。
该保障覆盖本领域函数，外部宿主持久化不在本函数契约内，后续适配不得在校验外持久化迟到快照。

## 红灯与实现

最小接口保留原有快照更新行为，并新增私有检查后barrier接缝。
[红灯](task42/red.out)：4项实际执行，1通过/3失败/0忽略，退出101。
两个并发用例由另一线程在检查后执行Tombstone/完整删除，具体失败为共享raw holder被迟到报告修改；
另一个用例失败为并发Removing被旧Serving快照覆盖。并非缺符号、零测试或人为破坏已有实现。

removed_store.rs::handle_checked改用真实StoresInfo::patch_store锁下重读当前状态，
在Removed/缺失时跳过raw/time/rolling/slow/read与cold入队，避免复活成员及发布迟到统计。
活动store在同一临界区内更新raw/time并接纳统计任务，保留最新生命周期metadata。
健康过滤移到锁外避免重入。公开handle_store_heartbeat签名保持不变，
独占BasicCluster已约束当前外部调用；内部共享访问接缝额外检验注册表的提交保护。
无新依赖、feature、Cargo.lock或存储/调度边界变更。生产文件有版权和相邻说明。
无持久化入口/新后台任务；不会触发MemoryStore或Etcd写入，未将宿主存储保证纳入领域函数。

## 最终验证

入口：`bash docs/rust/state/task42/slot.sh python3 docs/rust/state/task42/validate.py`。
[逐命令统计](task42/summary.json)、[阶段输出](task42/validation.out)及validation-0.out至validation-6.out保存输出。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 成功，无范围外Rust差异 |
| `cargo test -p astersql-pd-cluster --locked --lib state_task_42_removed_store -- --nocapture` | 0 | 4通过/0失败/0忽略 |
| `cargo test -p astersql-pd-cluster --locked` | 0 | 12通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 成功 |
| `cargo fmt --all -- --check` | 0 | 成功 |
| `cargo check --workspace --locked` | 0 | 成功 |
| `cargo test --workspace --locked` | 0 | 1236通过/0失败/1既有忽略 |

既有忽略为tso_processes；本任务无忽略。完整基线实际验证MemoryStore和EtcdMetaStore契约，
不将通用后端契约冒充本函数的持久化实现。已审查etcd_fixture_test.rs和
scripts/rust-etcd-fixture调用链，仅构建既有Go夹具，GOFLAGS=-mod=readonly，
无failpoint依赖；proto/build.rs从固定模块ZIP生成Rust到OUT_DIR，不写Go源码/模块。
复用夹具EOF取消并wait；测试显式join移除线程与HotStat.shutdown，FIFO快照屏障确认最终热点为空。

oracle、红灯及完整验证均按mkdir动态领取**槽位2**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
[资源核验](task42/resources.json)：自有子进程全部结束，槽位锁已释放，缓存保留；未触碰他人槽位1。
[Go只读核验](task42/readonly.json)及[重放检查](task42/readonly.py)：1101项清单与哈希均等于起点，
Git状态为空，无新增Go/模块文件；两份指定plan.md未修改，用户原有计划变更保留。
`git diff --check`通过，新增文档链接可解析。未运行会改写Go的make/failpoint入口。

状态：已完成。仅完成本领域的迟到心跳保护，非整个主题或持久化适配迁移完成。

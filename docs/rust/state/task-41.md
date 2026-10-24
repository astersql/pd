# 任务41：存储心跳状态更新

实施前审查：依赖 [task-40.md](task-40.md) 已完成，累计基础能力可用。
根AGENTS、Copilot、ADR、依赖基线、移植指南及两份只读plan已阅读；没有更深AGENTS。
起点commit与1101项Go/模块哈希、Git状态、只读plan哈希见 [before.json](task41/before.json)。
Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0；cluster.go/cluster_test.go无差异。

## 实施前 Go oracle

复用任务25已编译的ARM64/Linux调试二进制，SHA256
8781f08225e2b8b746217d79fe78e3ff072369cc8ae8941cc8d616a8a77986d9。
只读审查TestStoreHeartbeat/TestFilterUnhealthyStore、newTestScheduleConfig、
newTestRaftCluster、InitCluster、setStore、内存storage、TestMain/goleak路径。
选中测试不启用或依赖failpoint，无生成、模块修改或源码写入。
HandleStoreHeartbeat的network-slow failpoint分支不被该输入触发。
命令经 `bash docs/rust/state/task41/slot.sh` 领取槽位后运行：
`docker run --rm -v "$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/cluster-oracle41.test -test.run '^Test(StoreHeartbeat|FilterUnhealthyStore)$' -test.v`。
[运行输出](task41/oracle.out)：2通过，退出0，发生在Rust实现前。

原事件：3个stores/3个regions；先报未知store返回错误，注册后报告capacity=100、
available=50、region_count=1，lastHeartbeat非零、原始stats相等；响应NodeState。
read interval=(0,10)秒；region1/read_bytes=9999998/read_keys=9999999/query.get=9999997，
3次热报告后degree>=3的peer1负载=[999999.8,999999.9,999999.7,0]。
一次空peer报告使其cold，随后热报告恢复；连续3次cold后须再3次hot才能恢复degree3。
store改Removed后成功但不重新发布stats，rolling条目被清理。
运行原测试也覆盖limiter及元数据持久化；这些不属于本批次状态领域边界。

源码补充契约（不冒充原测试运行差分）：peer不存在region或store peer则跳过，
但reported集合仍记region ID；query=get+scan+coprocessor（uint64 wrapping，不含GC）；
CPU=unified_read * interval传入8维累积loads，统计层再除interval，scheduler CPU不计。
缺interval默认(0,0)，end-start按uint64 wrapping；Cold在所有Read后入同一FIFO。
独立调度服务时不观察stats/slow/hot，只更新core状态。领域时钟由宿主传Unix纳秒。

附加运行快照（亦在实际实现前）：[Delve输入](task41/oracle.commands)、
[输出](task41/oracle-debug.out)。在既有TestStoreHeartbeat首次hot调用前只改进程内存：
scan=2、coprocessor=3、GC=100；用既有WrittenKeys/WrittenBytes两段uint64内存承载
CPUStats={UnifiedRead:7,Scheduler:999}，Delve通过uintptr地址转换设置指针，未改源码。
真实HandleStoreHeartbeat在CheckReadPeerAsync入口给出interval=10、
loads=[9999998,9999999,10000002,0,0,0,70,0]。这证明查询求和、GC排除及CPU*秒接线。
初期Delve方法调用在ARM64上panic，后改为纯set输入；initial/second/third输出保留，
最终快照无命令错误。这是调试运行快照，未宣称修改输入后的原测试全通过。

[红灯](task41/red.out)：最小可编译接口始终返回StoreNotFound；5项实际执行，
0通过/5失败/0忽略，退出101，已注册store1却返回StoreNotFound(1)。
此前 `cargo fmt --all`、`cargo check -p astersql-pd-cluster`成功并维护锁文件；
红灯使用--locked，不以缺符号或零测试替代行为失败。

补充过滤快照：[缺失region](task41/oracle-missing-region-final.out)输入region99，
真实Go region=nil、reported={99}，跳过read并到达Cold；
[缺失peer](task41/oracle-missing-peer-final.out)输入已注册store3/region1，
peer=nil、reported={1}，同样跳过read并到达Cold。
[重放脚本](task41/oracle-debug.sh)只对既有二进制进程内存set，Delve exit终止debuggee，
Docker --rm回收容器，不写Go或模块文件。过滤补充快照在初版Rust后采集，
原测试与CPU/query实际快照在实现前已采集，时间顺序不混称。

## 实现与兼容证据

新增canonical astersql-pd-cluster，仅注册workspace member/dependency和对应lock项，
复用既有版本，无新增feature。store_heartbeat.rs公开handle_store_heartbeat与StoreNotFound。
以独占BasicCluster接收已有store状态，原错误文本、raw stats共享holder、lastHeartbeat
元数据脱离、Removed no-op、rolling健康清理、SlowStat和读热点接线均使用真实依赖。
None统计为独立调度服务路径；不实现注册/下线、持久化、limiter配置或调度反馈。
生产文件均有版权及相邻rs.md；测试独立且显式注册。

初次实现后 [green.out](task41/green.out)3通过/2失败，退出101，暴露测试误把
Rust现有异步快照的圆整GetLoad当作Go原测试的未圆整Loads。只读复核
hot_peer.go:GetLoad/Clone与现有Rust契约，保持依赖API；抽出实际生产peer_loads，
增加真实HotPeerCache.run_task原始速率断言。未降低小数值精度标准：
8维输入逐项等于Go调试快照，真实缓存loads逐项等于Go原测试断言
[999999.8,999999.9,999999.7,0]和CPU扩展[999999.8,999999.9,1000000.2,7]；
既有异步快照则按其去噪契约得到[1000000,1000000,1000000,0/7]。
这一区别属于复用的统计查询API，未声称Rust圆整快照与Go原始Loads完全相同。
[修正后绿灯](task41/green-final.out)6通过，退出0；后补reported缺peer不误冷却、
关闭队列仍更新core两项，最终共8项。

8项行为覆盖：已有/未知store及原错误、全量raw替换/重置、时间和旧快照共享边界；
Go连续hot/cold恢复序列；CPU/query及缺失region/peer过滤；Removed不更新及rolling移除；
None interval、倒序wrapping interval过滤、独立服务；原始累积输入和缓存小数速率；
仍reported却缺peer不冷却，后续不报告才冷却；已关闭队列不拒绝core报告。
成功路径都校验最终状态，未mock统计或索引；异步测试通过真实FIFO查询barrier，
并显式HotStat.shutdown等待其hot/bucket worker取消join（关闭队列场景重复shutdown安全）。
本任务无持久化与监听，MemoryStore/EtcdMetaStore只由workspace基线真实契约验证。
已审查复用etcd_fixture_test.rs只在槽位构建既有fixture、EOF关闭和wait，
proto/build.rs只读固定ZIP并在OUT_DIR生成；无Go源改写、注入或私有Tokio runtime。

## 最终验证与资源

入口 `bash docs/rust/state/task41/slot.sh python3 docs/rust/state/task41/validate.py`。
[逐命令统计](task41/summary.json)、[阶段输出](task41/validation.out)、
validation-0.out至validation-6.out保留全部结果。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化成功，无范围外Rust差异 |
| `cargo test -p astersql-pd-cluster --locked --lib state_task_41_store_heartbeat -- --nocapture` | 0 | 8通过/0失败/0忽略 |
| `cargo test -p astersql-pd-cluster --locked` | 0 | 8通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查成功 |
| `cargo fmt --all -- --check` | 0 | 检查成功 |
| `cargo check --workspace --locked` | 0 | 检查成功 |
| `cargo test --workspace --locked` | 0 | 1232通过/0失败/1既有忽略 |

既有ignored为tso_processes，需独立进程二进制；本任务无忽略。基线真实执行
MemoryStore/EtcdMetaStore契约，无新增后端或夹具。`git diff --check`退出0，新增文档链接均可解析。

所有阶段按mkdir动态领取共享槽位，未使用独立target目录。实际使用2和3：
- 槽位2：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`，
  oracle、bootstrap、首次实现验证及最终完整验证。
- 槽位3：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-3`，
  oracle在槽位2运行时的红灯与修正后6项聚焦绿灯。
源码修改、Cargo.lock维护、workspace fmt均串行；oracle调试只用既有二进制，
不修改共享源文件或执行Cargo。每个阶段的槽位在输出首行，锁owner含任务/会话/PID。
[资源核验](task41/resources.json)：所有自有槽位产物进程已退出、自有锁2/3已释放、
缓存保留；未操作既有槽位1锁。HotStat测试显式shutdown取消并join，etcd夹具退出并wait。

[只读核验](task41/readonly.json)：起点1101项Go/模块文件清单、内容与Git状态完全一致，
聚合SHA256 `ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`；
两份指定只读plan.md哈希相同，无新增Go文件，用户原有计划变更保留。

状态：已完成。仅完成已有store的心跳状态领域边界，不表示完整服务或整个主题完成。

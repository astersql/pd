# 任务51：双向同步流适配

实施前依赖：task-50.md 已完成，累计依赖见各前置记录。读取根AGENTS、Copilot、
ADR、移植指南、依赖基线及两份只读plan，无更深AGENTS。
起点commit、全部Go/模块逐项哈希、状态和plan哈希见 task51/before.json。

实施前必要运行oracle复用 task50/oracle.out 和 task-50.md 的真实Go SyncRegions
全量首帧、空completion及原始wire fixture；task36/oracle.out 的五个已执行Go测试
覆盖独立索引、慢发送、失败隔离、串行增量和keepalive；task40 的生命周期运行证据。
源码基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0，协议f8ad079c8e9812ed5900b0c12b5f1d97171464c4。
只读审查server/grpc_service.go SyncRegions、pkg/syncer/server.go Sync/bind/replay/Send、
client.go和相邻测试。源码推导：请求0/未来/过期索引走全量；同名重绑取消旧流；
完整同步的位置从0开始，空completion携带历史边界；客户端断开取消当前发送。
不将源码审查冒充新运行差分，新增Rust网络生命周期故障证据单独记录。

## 实现、失败证据和字段终态

[red.out](task51/red.out)：最小可编译接线运行1项，退出101，0通过/1失败；
全量首帧regions.len实际0、期望1。listener、领域worker在断言前已关闭并join。
[green.out](task51/green.out)同一断言通过。此前接口登记过程的编译错误不计行为证据。
[terminal-red.out](task51/terminal-red.out)追加真实网络退出测试：正常EOF错误返回Retry，
期望Complete，1项实际执行并失败；修复EOF/错误header处理后通过。
[large-payload-first.out](task51/large-payload-first.out)保留一次测试输入问题：使用
加密metadata使既有Storage返回save_errors=1，不能当成功路径。随后改成512KiB键，
通过实际HTTP/2流控制造背压而不越过存储能力边界。

新增server/sync_transport.rs与相邻说明/独立测试，lib显式登记。
`PdServices::region_sync_service`默认None；`SyncGrpc`只扩展SyncRegions并转发原有
六个方法，`grpc.rs`按任务要求保持只读。server没有到runtime的生产反向依赖。
`RegionSyncer::{new,publish,stop}`复用共享BasicCluster和唯一Downstreams历史；
宿主通过publish提交已经接纳的区域变化，cluster gate串行化准备/绑定与发布。
必要领域补充只有Downstreams::prepare_sync、BootstrapFrame和公开wait：复用既有
plan_replay/FullSnapshot，不复制历史或应用算法，不将proto/tonic带入领域。
全量按0/1000位置分批，完成帧携带捕获历史边界；该边界后的变化从独立下游索引发出。
初始发送gate防止live帧抢在快照前；一槽channel限制实时帧排队；没有锁跨越网络I/O。
接纳channel发送是本地transport acceptance，不冒称远端应用确认。

每流使用现有领域worker和受宿主管理的supervisor；断开/同名重绑/发送失败/超时/
宿主取消都会取消并join worker。stop保留未完成supervisor句柄，关闭未来被丢弃后
可继续等待；完成supervisor在下一次open中通过真实poll回收，或在stop中join。
返回流终止状态Unavailable，与Go停止路径一致；不启动脱离宿主的重试或私有runtime。
默认send timeout10秒，测试显式2秒，故障边界显式10毫秒。
`SyncFollower`实现既有SyncAttempt：真实tonic connect/request/receive，经codec进入
真实ClientApply/Storage；保留history与cache，只重置每条流的readiness。
错误header（包括code0）结束当前流并Retry，正常EOF Complete；leader变化由宿主
cancel/join旧SyncLifecycle后选择新endpoint。Storage/keypath从已有dev依赖提升到
production workspace依赖，不新增版本、feature或Cargo.lock依赖项。

最终9项聚焦行为：
- 完整全量首帧与completion；真实域容器初始ID1/version1/history1。
- fast从history1开始，slow从0开始且暂停读取；连续7个512KiB键变化制造多MiB真实
  HTTP/2背压，fast每步history2..8，slow恢复后同为ID1/version8/history8，全部字段相同。
- 同名重绑终止旧流且不误删新绑定；断开后追加version3，从未确认history2重连，
  最终version3/history3，完整canonical cache参与校验。
- 真实关闭channel发送失败；满channel触发10ms发送deadline，失败worker索引仍0，
  注册表清空；网络宿主stop后Unavailable，listener显式cancel/join。
- 两个真实listener与两个源域容器，SyncFollower经SyncLifecycle切换endpoint，
  cancel并join旧代后从history1重连，最终version2/history2；停止后绑定归零。
- 1001个真实非重叠区域，快照offset0/1000；首批后publish ID1001/version2，
  全量completion history1002后增量到1003，最终1001个区域且ID1001/version2。
- 首请求尚未发送时stop，真实RPC返回Unavailable，无绑定或后台worker残留。
- 真实EtcdMetaStore follower全量+增量，最终history2、cache version2；重新读取
  Storage.load_region得到同一version2元数据，fixture EOF关闭并wait。
- 控制的网络错误header0后保持流不关闭，follower立即Retry且history9/cache原值不变；
  正常EOF Complete。仅控制传输故障，不mock领域状态或成功结果。

这只是可选同步流适配；宿主接线须遵守publish admission gate及stop责任，
未声明完整服务切换、调度接入或新的持久化引擎。MemoryStore与EtcdMetaStore均有
本任务终态/既有领域回归；hist index始终独立于MetaStore revision。

## 最终验证与退出

统一入口：`bash docs/rust/state/task51/slot.sh python3 docs/rust/state/task51/validate.py`。
[summary.json](task51/summary.json)与[validation.out](task51/validation.out)及
validation-0.out至validation-6.out保存精确命令、退出码与有效测试数量。
首轮workspace1301通过/1既有忽略见[first-summary.json](task51/first-summary.json)；
修复EOF/header并追加第9项测试后重新执行全部必需命令，最终如下：

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 成功 |
| `cargo test -p astersql-pd-server --locked --lib state_task_51_sync_transport -- --nocapture` | 0 | 9通过/0失败/0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 48通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 成功 |
| `cargo fmt --all -- --check` | 0 | 成功 |
| `cargo check --workspace --locked` | 0 | 成功 |
| `cargo test --workspace --locked` | 0 | 1302通过/0失败/1忽略 |

唯一既有ignored为tso_processes，本任务无ignored或零测试验收。
MemoryStore/EtcdMetaStore、独立下游索引、重连、失败与cancel/join均有真实终态证据。
已复核proto/build.rs只向OUT_DIR解压/生成，etcd_fixture_test.rs仅构建既有Go夹具，
脚本入口不生成或改写Go/模块、不需要failpoint。夹具EOF关闭并wait，异常kill后wait。
所有子进程继承GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0、GOARCH=arm64。
本任务未运行Go测试、make/failpoint启停、Go生成、格式化或模块整理命令。

红灯、首次绿灯、扩展验证和两轮完整验证均动态领取槽位2，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
[resources.json](task51/resources.json)记录本槽位锁释放、缓存保留、无活跃测试/fixture进程；
各loopback listener及follower lifecycle均显式cancel/join，服务stop后注册表为空。
其他槽位的锁/进程未操作。只清理本任务临时文件，保留共享缓存。

[audit.py](task51/audit.py)与[readonly.json](task51/readonly.json)确认1101项Go/模块文件
集合、逐项SHA256及Git状态与起点相同，聚合SHA256
`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`；
全部已记录plan.md哈希相同，grpc.rs无差异，workspace fmt没有范围外Rust改动。
Markdown链接与git diff检查通过。证据日志只规范制表符与行尾空白，保留真实失败信息。

状态：已完成。仅完成批次51同步流适配，未将其等同整个迁移主题完成。

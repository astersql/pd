# 任务40：同步取消与资源回收

起点见 [before.json](task40/before.json)；依赖 [task-39.md](task-39.md)
已完成。仅根AGENTS适用。Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0；
server.go/client.go/server_test.go相对基准无差异。

## 实施前oracle

只读审查server.go的Sync、sendRegionSyncResponseWithOptions、closeAllClient、
regionSyncStream双层发送锁，以及client.go的Start/StopSyncWithLeader和retry等待；
server_test.go:321–659四项指定测试与TestMain、LevelDB/内存存储夹具不依赖failpoint。
复用任务37既有ARM64调试二进制，SHA256
1b5427ae96908f6a42a5e4ba3946e7054a807e3370f86edbf007645a30939d2e。
[真实运行输出](task40/oracle.out)四项全PASS、退出0；运行发生于Rust实现前。
命令：`bash docs/rust/state/task40/slot.sh bash -c 'shasum -a 256
"$CARGO_TARGET_DIR/syncer-oracle37-arm64.test"; docker run --rm -v
"$CARGO_TARGET_DIR:/build:ro" alpine:3.22 /build/syncer-oracle37-arm64.test
-test.run "^Test(SyncExitsWhenRegionSyncerStops|SyncExitsWhenContextCanceledBeforeRequest|CloseAllClientTimesOutBlockedSend|SendRegionSyncResponseSerializesStreamSendAfterTimeout)$" -test.v'`。

事件/字段：建立请求并收到全量completion后cancel，Sync返回Unavailable且绑定清空；
首请求前cancel也返回Unavailable；closeAll在Send阻塞后10ms超时退出、绑定清空；
index=10记录一个区域后Send超时，第二个Send必须等在途发送结束，最终调用数2。
时间单位为Duration，索引为history sequence，与MetaStore revision无关。
Go阻塞测试在末尾人工释放Send；Rust传输边界要求丢弃future取消I/O，超时后关闭该流，
不在同一流再发送关闭帧，避免留下detached worker。此为满足本任务资源约束的明确实现
差异，不声称Go已经join其阻塞Send。领域错误Closed/Timeout/Downstream交给server适配
映射Unavailable，领域不依赖tonic/protobuf。重试取消为client.go源码契约扩展，不冒充
额外Go运行差分。指标保持pd_region_syncer_status Gauge/type及三个history索引标签；
宿主Registry显式注册，无全局状态。

## 实现与行为证据

新增 `SyncLifecycle::{new,start_client,start_downstream,cancel,stop}`，只组合现有
Downstreams/DownstreamTask及宿主Handle。首请求/连接/收流的整个SyncAttempt与retry
sleep均有优先取消；错误重试、EOF/leader-loss Complete退出。所有attempt串行，传输
future必须可通过drop取消；实际协议适配仍留server任务，不引入proto/tonic。
DeadlineSender给每次Send设限；0使用10秒默认；超时关闭绑定并返回send错误，不推进
索引、不启动关闭帧发送、没有detached Send。stop先取消全部再逐一join，即使某worker
panic也继续回收其他worker；重复stop安全，Drop只发取消，宿主必须await stop。
未完成join通过可变引用保留在owner，临时丢弃stop future不会丢失pending handle。

SyncMetrics仅向宿主Registry注册固定GaugeVec，标签句柄缓存。为落实Go指标语义，
必要局部接线扩展HistoryBuffer::attach_metrics及PersistentHistory::persist：sync_index
记录最后分配值（非next），reset/reload不更新；first/last在持久化尝试前更新，包括
写失败。既有下游组件与历史算法不重写；所有生产文件有版权及相邻rs.md。
仅新增workspace既有prometheus引用及对应Cargo.lock依赖项，无新版本/feature。
初始lock维护 `cargo check -p astersql-pd-syncer`退出0；后续验证均--locked。

[断言红灯](task40/red.out)：最小接口复用已有取消/join，暂缺发送期限；7项运行，
6通过/1失败/0忽略，退出101。明确失败 `send deadline must remove the blocked stream`；
测试在断言前已cancel/join，无泄漏，不以编译错误或零测试冒充红灯。
[首次扩展绿灯](task40/green.out)：11通过/0失败/0忽略，退出0。

最终11项聚焦覆盖：3个同时阻塞Send全部cancel/join并销毁sender；10ms期限关闭
阻塞流且20次broadcast仍仅1个Send/maximum=1；零超时实际采用10s（暂停时钟9s
仍active、10s退出）；首poll前取消calls=0；首请求pending时取消；3600s重试等待
取消且calls=1；推进时钟1s后仅第2次串行重试；Complete释放资源；worker panic
收集JoinError但仍回收阻塞sender；真实MemoryStore与EtcdMetaStore各一项组合行为。

两个后端都实际持久化historyIndex=11，history(first,next)=(10,11)，指标
(sync_index,first_index,last_index)=(10,10,11)；reset到700保留sync_index=10。
真实ClientApply应用region42、变为running并检查BasicCluster、Storage及next=11；
继续等待收流。MetadataWatch从真实后端加载a=ready后等待next；stop同时取消并join
这两个worker，active=0/drop=1，channel接收端销毁，region42仍可读，独立history
仍为700；后端继续Put/read a=after成功。结果不mock领域/存储/索引。

Go指标更新顺序与重试策略为只读源码契约扩展；四项指定生命周期Go测试有实际运行
证据。Rust不是Go协议服务器，本任务不声称完整SyncRegions服务切换或协议验收。
fixture复用已有scripts/rust-etcd-fixture源码，Go build只在槽位输出二进制，EOF
关闭并wait；proto/build.rs只读固定模块ZIP，在OUT_DIR生成Rust。全部子进程继承
GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0、GOARCH=arm64，无failpoint注入。

## 最终验证

入口 `bash docs/rust/state/task40/slot.sh python3 docs/rust/state/task40/validate.py`。
[summary.json](task40/summary.json)保存确切命令、退出码和有效数量，
[validation.out](task40/validation.out)为阶段摘要，validation-N.out为完整输出。
所有Cargo阶段动态领取槽位2：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 检查成功 |
| `cargo test -p astersql-pd-syncer --locked --lib state_task_40_lifecycle -- --nocapture` | 0 | 11通过/0失败/0忽略 |
| `cargo test -p astersql-pd-syncer --locked` | 0 | 85通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查成功 |
| `cargo fmt --all -- --check` | 0 | 检查成功 |
| `cargo check --workspace --locked` | 0 | 检查成功 |
| `cargo test --workspace --locked` | 0 | 1224通过/0失败/1忽略 |

workspace既有ignored为tso_processes，需独立进程二进制；本任务无新增忽略，
聚焦/包级均无失败或忽略。`git diff --check`退出0。
[只读核验](task40/readonly.json)：1101个Go/模块文件清单、内容与Git状态前后相同，
聚合SHA256 `ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`；
两份指定只读plan.md哈希完全相同，无新Go文件。用户既有计划变更保留。
[资源核验](task40/resources.json)：所有slot2产物使用进程已退出、锁已释放、缓存保留；
既有slot1锁/进程未操作。测试证明cancel后join完成、active=0、sender/attempt被销毁，
etcd fixture EOF关闭并wait；没有私有runtime或新后台进程残留。

状态：已完成。只交付同步生命周期边界，不表示整个主题或协议服务切换完成。

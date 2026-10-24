# 任务39：元数据监听恢复契约

起点 `8a49cd08190ffcbec3db5fdf9064fe88b6a9104c`；批次38必要证据见
[task-38.md](task-38.md)，依赖满足。仅根AGENTS适用。

## 实施前 oracle

基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；etcdutil.go 相对基准无差异。
只读审查 LoopWatcher load/watch/reloadAfterCompaction 及相邻测试。
[实际运行输出](task39/oracle.out)：六个子测试全部通过，退出0。
选定测试为 TestCallBack、TestWatcherAtomicLoadCallbacksDiscardIncompleteLoad、
TestWatcherConsistentLoadUsesSingleRevision、TestWatcherLoadCancelsInFlightRequest、
TestWatcherReloadsAfterCompactionWhenEnabled、TestWatcherUsesCustomCompactionReload。
这些测试及其SetupSuite/TearDownSuite/TestMain不需要failpoint；未执行其余注入测试。
既有Go源通过 `GOOS=linux go test -mod=readonly -c -o
"$CARGO_TARGET_DIR/etcdutil-oracle39-arm64.test" ./pkg/utils/etcdutil` 编译，
Docker alpine:3.22运行既有测试二进制；完整命令见[oracle.sh](task39/oracle.sh)。
二进制SHA256 `38e0609a8a3bf728983b565dc8f09df513b3a16e6ac706468e71d6d6b3e0b979`。
构建继承 GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0、GOARCH=arm64。
没有新增/改写Go源码或模块文件。fixture既有Go入口只在槽位写WAL/ready和二进制；
proto/build.rs只读模块ZIP，在OUT_DIR生成Rust协议。未执行make或注入工具。

字段契约：a,b,c=old，首页后同事务a,b=new；所有快照页仍为old，watch从
首页revision+1开始；Compacted后重读最新快照，删除旧快照残留键，再从新revision+1
接续。前两项为Go一致加载/压缩重载实际断言，删除对账及断连重放按已存在
ReconciledWatch/MetaStore契约扩展验证，不声称额外Go运行差分。
取消必须终止在途读取/观察，宿主join。revision单位为后端事务序列，history index
为独立区域历史序列，不相互赋值。region持久化已有ClusterPaths::region_path和
StorageEndpoint::save_region/load_region及私有codec；本模块观察不透明元数据字节，
不复制codec，不修改SyncRegions或自动把原始元数据写进区域缓存。

复用既有astersql-pd-etcdutil::ReconciledWatch，新增syncer本地原子状态接口。

## 实现与行为验证

新增 `MetadataWatch::{load,entries,next_revision,advance}`，维护独占借用下的原始
元数据视图；恢复和分页算法复用唯一ReconciledWatch，没有平行存储实现。
默认features，仅登记既有etcdutil与tokio-util依赖；Cargo.lock增加两个既有依赖引用，
无版本变化。生产文件有版权及相邻rs.md；测试独立且显式登记。

[红灯](task39/red.out)：最小可编译接口完成加载但未发布状态、未应用事件；
0通过/4失败/0忽略，退出101，具体断言 `complete initial metadata snapshot`
得到0而预期3；空快照后事件未应用得到0而预期1。不是未定义符号或零测试。
[首次绿灯](task39/green.out)：同4项测试全部通过，退出0；之后增加真实etcd重启测试。

最终聚焦5项：两个后端各自执行相同的分页/重连/压缩/取消序列与空快照/分页压缩
序列，另1项使用真实etcd进程stop/start和同WAL恢复。

- 初始同事务a,b,c=old；首页后真实事务a,b=new。分页结果仍全old，cursor=initial+1；
  第一次advance取回两项new，cursor=initial+2。历史索引始终是独立HistoryIndex(700)。
- Delete a/Put d同事务；丢弃实际收到的IO响应，Unavailable不改变状态/游标，
  active stream计数归零；下次从同一未确认revision重连，最终a消失、d=live。
- 新观察器尚未打开stream时Delete b,c/Put d=recovered；额外写prefix外键推进压缩点。
  compact后重载第一次读响应丢弃：Unavailable保持旧快照及cursor；重试只剩d，
  next=新快照revision+1，d.mod_revision仍是实际写入revision而非最新header。
  再Put e，立即从重载边界继续观察成功。
- 真实watch已进入next后通知宿主cancel；8秒期限内join宿主worker，返回Cancelled；
  新令牌仍Cancelled，active stream=0；HistoryIndex未改变。
- 空prefix视图后Put可被观察；另一prefix加载分页时真实写入并compact首页revision，
  返回精确Compacted.requested且不发布部分状态，重试得到3键的完整新快照。
- 真实etcd停止时Unavailable保留视图/游标；同WAL启动后Delete a/Put b，重连得到
  只含b=two的终态，cursor=commit+1，最后取消关闭。

运行入口 `bash docs/rust/state/task39/slot.sh python3 docs/rust/state/task39/validate.py`。
所有阶段从根动态领取槽位2：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
[summary.json](task39/summary.json)记录逐条确切命令、退出码与有效数量；
[validation.out](task39/validation.out)为阶段摘要，各validation-N.out为完整输出。

| 最终命令 | 退出码 / 有效结果 |
| --- | --- |
| cargo fmt --all | 0 |
| cargo test -p astersql-pd-syncer --locked --lib state_task_39_metadata_watch -- --nocapture | 0；5通过/0失败/0忽略 |
| cargo test -p astersql-pd-syncer --locked | 0；74通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 |
| cargo fmt --all -- --check | 0 |
| cargo check --workspace --locked | 0 |
| cargo test --workspace --locked | 0；1213通过/0失败/1既有ignored |
| git diff --check | 0 |

初次维护Cargo.lock使用 `cargo check -p astersql-pd-syncer`（退出0），先fmt再红灯；
随后全部测试使用--locked。workspace既有ignored为需独立进程二进制的tso_processes，
与本任务无关；无新增忽略测试。无后台生产任务，测试worker均已cancel并join；
etcd夹具EOF关闭并wait子进程，真实stop/start场景亦已退出。
[资源核验](task39/resources.json)确认本任务槽位2锁已释放，使用该槽位的测试和
fixture进程均已退出，缓存保留；原有槽位1锁与进程未操作。

[只读核验](task39/readonly.json)：1101项Go/模块文件清单、内容和Git状态前后相同，
聚合SHA256 `ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`；
两份指定只读plan.md哈希相同。用户已有计划变更均保留，不纳入提交。

状态：已完成。只交付元数据监听恢复契约，不表示整个区域同步主题完成。

# 任务52：心跳到同步状态联通

依赖 task-51 已完成，累计前置沿其证据；已读取根规则、Copilot、两份只读plan、ADR、移植指南、依赖基线，无更深AGENTS。
起点commit、Go/模块逐项SHA256与Git状态、plan哈希见 [before.json](task52/before.json)。

## 实施前 oracle

Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0，当前相关Go源码未变。
复用已有真实运行证据： [心跳/split/merge](task46/oracle-final.out)、
[bucket版本与继承](task45/oracle.out)、[store统计](task41/oracle.out)、
[同步客户端](task37/oracle.out)、[同步流](task51/validation-1.out)。
Go二进制与输入来源、原测试运行方式、字段断言见 task-41/45/46/37；不把Rust输出称作新Go运行。
只读审查 server/cluster/cluster.go:1160起、1344–1492、cluster_worker.go、pkg/cluster/cluster.go、
pkg/statistics/hot_stat.go、pkg/syncer/server.go/client.go及相邻Go测试。

固定序列：store1已注册→全范围region1 epoch2/2→相同报告→旧epoch拒绝→bucket v2→
region1 epoch3（继承bucket）→右半region2 epoch4→左半region1 epoch4→合并region1 epoch5。
每步等待effects与下游应用后读取root/subtree、异常分类、HotStat各缓存、KV和follower。
新区域/epoch/范围变化更新KV与history；重复和stale不推进history/KV；重复仍可喂入热点统计，
这与Go在guide前CheckRegionFlowAsync一致。bucket单独报告只更新本地worker/cache，
后续需要同步的region心跳才携带bucket；bucket版本不充当history index。
flow是原始累计字节/keys，interval秒；region size是MiB；store时间是Unix纳秒，collect时间Unix秒。
follower按Go只消费meta/leader/四项flow/bucket，size/keys/down/pending不在同步子集，不能要求完整RegionInfo相等。
split先右半替换全范围，再补左半；merge删除被覆盖区域的cache、异常分类和KV，热点沿原TTL/继承规则。
同步history与MetaStore revision独立。源代码推导与上述已保存运行证据分别标记，不声称新的逐wire差分。

## 实现与行为证据

[red.out](task52/red.out)：可编译接口实际运行1项，0通过/1失败，退出101；
accepted region reports must feed canonical write statistics：实际0、预期1。
该测试先停止并join runners/HotStat，再断言。red-lock.out记录新增dev依赖后锁文件未维护的错误，
red-compile.out记录runner迭代缺mut的编译错误；两者不计行为红灯。
[green.out](task52/green.out)3项通过，[wire.out](task52/wire.out)真实RegionSyncer1项通过。
wire-compile.out记录visitor需mutable borrow的初次接口错误，已修正并重新验证。
首次generate-lockfile --offline附带更新pulldown-cmark，已恢复原版本/摘要；最终锁差异仅5条既有workspace依赖边。

StatePipeline复用store/bucket admission、submit_region_heartbeat与HotCache.check_region_flow_async，
返回原effect receipt；新StateSnapshot复制raw store stats、store角色计数、异常bitmap、labels及HotStat FIFO快照。
不复制统计/索引算法，不调用operator/checker/scheduler，不引入protocol到领域层。
region成功接纳后喂热点（重复也喂入），stale不进入任何副作用；宿主串行接纳，
不宣称复刻Go并发第二次检查拒绝之前可能已经入队的热点任务。
快照要求宿主quiesce并wait effects，未声称跨并发cache的原子revision。

必要局部接线扩展至pkg/server/sync_transport.rs：RegionSyncer::for_heartbeat_state与downstreams，
通过私有SyncSource visitor在原HeartbeatEffectsState锁内捕获全量和bind；
原new/publish仍使用BasicCluster。sync effect在同一state gate内append/broadcast，锁序state→history，
无锁跨越网络I/O，StatePipeline与RegionSyncer实际共享一份root与一份history。
该模式只由pipeline发布，不再次publish；默认服务切换和协议路由没有改动。

4项最终聚焦行为：
- MemoryStore与真实EtcdMetaStore源端，逐步接纳region1 v2→bucket v2→region1 v3→右半region2 v4→左半region1 v4→merge region1 v5；
  canonical Downstreams帧经ClientApply进入独立MemoryStore副本，每步meta/leader/四项flow/bucket投影一致。
  history依次0..5，每步真实源/副本KV、root/subtree业务ref2、region角色数、分类bitmap和label快照校验。
  每步duplicate/stale保持root Arc、源KV revision和history长度不变，未产生下游帧。
  bucket-only仅本地version2/worker有值；v3后副本取得同一bucket。merge后两端cache/KV无region2，分类不再观察2。
- 同一region3次有效报告及store read报告，真实write/read peer都存在，raw容量100；
  返回快照改成999不改变live store。旧epoch、未知store不改变统计快照；关闭workers后snapshot错误而非部分结果。
- 真实HTTP/2 RegionSyncer直接挂接HeartbeatEffectsState，3次epoch更新后副本history3/version3/flow一致，
  duplicates不增加history；额外从0订阅全量，首帧读取同一root的version3、completion index3，未做publish缓存拷贝。
  增量和全量流均关闭；syncer.stop、listener cancel/join、全部runners stop与HotStat.shutdown显式等待。

额外只读重跑Go oracle：[脚本](task52/oracle.sh)、[输出](task52/oracle.out)，退出0，
9项原测试全部通过（StoreHeartbeat、BucketCompatibility、StaleBucketMeta、RegionHeartbeatHotStat、
BucketHeartbeat、RegionHeartbeat、RegionSizeChanged、HeartbeatSplit、OfflineAndMerge）。
复用原task41 Linux/ARM64二进制，SHA256 8781f08225e2b8b746217d79fe78e3ff072369cc8ae8941cc8d616a8a77986d9，
只读挂载具名Docker容器；重审TestRegionHeartbeatHotStat及既有TestMain、内存storage/SyncRunner调用链，
所选测试无需注入。该新增运行补强已复用证据，不把初次实现后重跑倒述为实施前新运行。
具名容器由trap删除，docker ps复核无pd-state52容器。未新增或修改Go工具/测试。

## 最终验证

入口：`bash docs/rust/state/task52/slot.sh python3 docs/rust/state/task52/validate.py`。
[summary.json](task52/summary.json)、[validation.out](task52/validation.out)及validation-0至8保存命令、退出码和测试数。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 无范围外Rust格式差异 |
| `cargo test -p astersql-pd-cluster --locked --lib state_task_52_state_pipeline -- --nocapture` | 0 | 3通过/0失败/0忽略 |
| `cargo test -p astersql-pd-cluster --locked` | 0 | 46通过/0失败/0忽略 |
| `cargo test -p astersql-pd-server --locked --lib state_task_52_state_pipeline -- --nocapture` | 0 | 1通过/0失败/0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 49通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 成功 |
| `cargo fmt --all -- --check` | 0 | 成功 |
| `cargo check --workspace --locked` | 0 | 成功 |
| `cargo test --workspace --locked` | 0 | 1306通过/0失败/1既有忽略 |

既有ignored为tso_processes，本任务聚焦无忽略，未以零测试或编译成功代替行为验收。
聚焦含源端MemoryStore/EtcdMetaStore与MemoryStore副本；真实Etcd副本适配与后端故障/退出由
既有task51及task46测试在server/cluster包级和workspace重新通过，不冒称新增聚焦测试覆盖了所有故障。

所有阶段动态取得槽位2：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
[resources.json](task52/resources.json)记录锁已释放、缓存保留、无自有活跃槽位产物进程。
既有etcd_fixture_test.rs→scripts/rust-etcd-fixture/main.go只读go build，产物在槽位；
proto/build.rs读取固定模块ZIP并仅写OUT_DIR。子进程继承GOFLAGS=-mod=readonly、GOPROXY=off、
CGO_ENABLED=0、GOARCH=arm64。fixture正常EOF关闭并wait，异常kill后wait；无私有Tokio runtime。
未运行Go测试构建入口、make、failpoint启停/注入、格式化、生成或模块整理。

[audit.py](task52/audit.py)与[readonly.json](task52/readonly.json)核对1101项Go/模块文件集合、
逐项SHA256及Git状态与起点相同，聚合SHA256
`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`；
所有已记录plan.md哈希不变。无范围外Rust格式改动，保留用户既有计划变更。
Markdown链接与git diff检查通过。日志只规范制表符与行尾空白，保留实际失败信息。

状态：已完成。仅完成批次52状态面联通，不声明整个迁移主题或服务启动切换完成。

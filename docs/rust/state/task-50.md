# 任务50：同步帧协议转换

依赖[任务49](task-49.md)及累计领域验收。起点和全部Go/模块逐项SHA256、Git状态、
只读plan哈希保存在[before.json](task50/before.json)。无更深AGENTS。
已阅读根规则、Copilot、ADR、移植指南、依赖基线和两份只读plan。

## 实施前oracle

Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0；审查pkg/syncer/server.go
buildSyncRegionResponse、syncHistoryRecordsLocked、fullSync及client.go响应处理。
协议固定kvproto f8ad079c8e9812ed5900b0c12b5f1d97171464c4。
复用既有native pd-oracle47二进制，不新增/构建Go工具。
[入口](task50/oracle.py)启动一次性Go服务，Rust真实tonic客户端
pkg/server/examples/sync_codec_oracle.rs使用自定义codec捕获Go直接返回的原始wire字节，
输出到tests/rust-compat/fixtures/state/task-50.json；不是prost重新编码冒充Go wire。
[实际输出](task50/oracle.out)保存全量首帧和空completion，Go进程TERM后wait退出0。

字段不匹配/leader缺失的必要运行证据复用[任务37](task-37.md)的fields-final.out与
mismatch-final.out：stats/buckets整体长度不匹配时整组忽略，leader逐位置读取且ID0无效；
错误header存在（即使code0）不应用。多批位置索引复用[任务34](task-34.md)与
[任务35](task-35.md)已执行的1001条Go测试，位置0/1000而完成携带历史1043。
源码补充契约与运行差分分开：发送端完整对齐四个附属数组，缺leader/bucket发空消息；
RegionStat四个counter不换算单位。bucket字段无stats（由Go GetBuckets生成）。
新协议现有down_peers/pending_peers字段Go syncer尚未消费；adapter保留并往返，
投影到现有SyncApplyResponse时按Go忽略，不将协议类型传入领域。

Go首帧实测：region1/epoch(2,2)/peer2/store1，written_bytes=1234、read_bytes=4567、
written_keys=23、read_keys=89，bucket(region1,version7,keys=[空,m,空])；start=0。
完成帧start=1，四个数组均空。测试逐字节比较原始Go输出与Rust发送端组装结果。
首次捕获使用零counter，随后用上述显式counter重新运行oracle，最终fixture来自后一次真实响应。

## 红灯与实现

[red.out](task50/red.out)先cargo fmt --all、cargo check -p astersql-pd-server --offline
维护已有workspace dependency边，然后运行指定--locked聚焦命令。
最小接口可编译，实际运行1项，0通过/1失败/0忽略，退出101：regions.len实际0、期望1。
[green.out](task50/green.out)同一真实wire断言通过。
扩展测试第一次仅因测试误用get_region_count而编译失败（[focus.out](task50/focus.out)），
已改为现有get_regions().len；该次不计行为验收。

新增sync_codec.rs及独立测试/相邻说明，lib显式注册，server仅增加既有syncer依赖，
测试增加既有keypath依赖，Cargo.lock仅两条边，无第三方新版本/feature。
SyncRequest/SyncResponse作为server owned envelope，复用core metadata、peer、bucket、
syncer SyncRegionStats/SyncResponseError及statistics BucketFlows，不引入第二份应用算法。
现有member crate持久化身份只保存adapter编码bytes，不具备这份member字段值类型，
本任务的SyncMember只负责协议字段的所有权，不实现成员发现/注册/选主。

history_to_proto/snapshot_to_proto/catch_up_to_proto复用已有SyncFrame/SnapshotBatch/
CatchUpFrame。发送端附属数组完整对齐，缺leader/buckets编码空消息；所有索引无隐式推进。
接收端原样保留不对齐数组和零leader，into_apply交由ClientApply执行Go长度与ID规则。
响应start_index用原始u64保留snapshot位置含义；ClientApply负责位置与历史的分类。
所有pinned字段均可往返，包括Go暂不消费的down/pending peers与非canonical bucket
stats/period；into_apply消费子集与Go一致。错误header保留code0及未知枚举。

7项测试覆盖实际Go wire往返/逐字节发送对比、header/member presence及全部字段、
极值/未知peer role/witness/encryption/flashback、全部bucket counters和毫秒period、
缺失leader、空keepalive、错误code0/2/999不改变缓存和历史，缺header可完成同步。
字段不匹配用真实MemoryStore/BasicCluster/ClientApply复现task37成功2/stale1、
history12、最终仅ID3：匹配时303与bucket7，不匹配时counter0/bucket缺失，leader缺失。
1001条真实领域全量快照经codec往返后进入ClientApply：offset0/1000，满量期间history0，
空completion1043后ready，再增量1043→1044；canonical存储与索引参与成功路径断言。
额外pinned字段极值/往返是schema/source-derived Rust验证，不冒称新增Go运行快照。
生产codec没有I/O、后台任务或私有runtime，不需要独立cancel/join；完整SyncRegions
服务路由、注册/发送生命周期仍由后续任务接线。


## 最终验证与资源

统一入口：`bash docs/rust/state/task50/slot.sh python3 docs/rust/state/task50/validate.py`。
[逐命令统计](task50/summary.json)、[总日志](task50/validation.out)及validation-0.out至
validation-6.out保存精确命令、退出码与测试计数。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 成功 |
| `cargo test -p astersql-pd-server --locked --lib state_task_50_sync_codec -- --nocapture` | 0 | 7通过/0失败/0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 39通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 成功 |
| `cargo fmt --all -- --check` | 0 | 成功 |
| `cargo check --workspace --locked` | 0 | 成功 |
| `cargo test --workspace --locked` | 0 | 1293通过/0失败/1忽略 |

workspace既有1项ignored为tso_processes，本任务无忽略。MemoryStore与真实EtcdMetaStore
契约均通过；codec无MetaStore I/O，不宣称新增持久化能力。
已审查pkg/proto/build.rs只向OUT_DIR解压/生成，pkg/storage/kv/etcd_fixture_test.rs与
scripts/rust-etcd-fixture/main.go只编译既有Go夹具，EOF关闭、wait，故障时kill/wait。
子进程继承GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0、GOARCH=arm64。
未运行Go测试、注入、生成、格式化或整理入口，所有Go/模块文件只读。

所有oracle、红灯、绿灯和最终回归阶段均动态领取槽位2，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
[resources.json](task50/resources.json)记录本阶段所有子进程结束、自己的锁释放、缓存保留、
无本槽位test/oracle/etcd进程、oracle临时数据删除，以及复用Go二进制SHA256。
其他槽位既有进程/锁不属于本任务，未改动或删除。
[只读核对](task50/readonly.json)确认1101项Go/模块清单、逐项哈希及Git状态与起点相同，
聚合SHA256为ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4；
全部已记录plan.md哈希相同。Markdown链接与Git diff检查通过，workspace fmt没有范围外改动。
日志只规范尾部空白和制表符，未删改失败信息或通过计数。

状态：已完成。仅验收同步帧协议转换，未将其等同完整同步服务切换。

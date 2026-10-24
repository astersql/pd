# 任务53：状态面差分与故障回归

依赖 [任务52](task-52.md) 累计 1–52 已完成。起点 commit、1101 项 Go/模块逐项
SHA256/Git 状态及 29 份只读 plan.md 哈希见 [before.json](task53/before.json)。
已读根规则、Copilot、ADR、依赖基线、移植指南和任务/整体 plan，无更深 AGENTS。

## 实际 Go oracle 与字段契约

基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；当前 cluster.go、syncer
server/client、core region.go 与 etcdutil.go 相对基准无差异。
复用任务47只读构建的 native ARM64 `pd-oracle47`，SHA256：
`446a9ce18ed5ac513d4cc66306cfe276db44cd2198db2d9e4e53d20520040523`。
没有新增/改写 Go oracle。原先 cluster 测试的 RegionSyncer=nil 只是该测试入口的限制，
完整既有服务具有真实心跳→KV→history→SyncRegions 链路，已据此取得联合运行输出。

入口 [slot.sh](task53/slot.sh) → [oracle.py](task53/oracle.py) →
[state_oracle.rs](../../../tests/rust-compat/state_oracle.rs)。使用隔离动态端口/数据目录，
[显式配置](task53/oracle.toml) 禁用 heartbeat concurrent runner 以匹配串行 admissions，
replicas=1、placement rules=false、schedulers=[]、hot threshold=0、flow divisor=1；
region metadata 使用 Etcd。Go 自带 history 的 Base 仍是独立 RegionStorage/LevelDB，
即使 use-region-storage=false 也不转为 etcd revision/key，未混用这两种序列。

实施前 [首轮真实输出](task53/oracle-first.out) 已取得 16 步每步联合状态；
[最终输出](task53/oracle.out) 补齐持续增量、断连追赶和真实服务重启。
[主 fixture](../../../tests/rust-compat/fixtures/state/task-53.json) 保存原始 protobuf
事件 bytes 和每步整个观测快照：HTTP RegionInfo、完整原始 StoreStats/StoreMeta、
读/写热点、bucket HTTP 返回值、KV RegionMeta，以及 full/live/catch-up 完整状态帧。
HTTP 未暴露的原始 bucket 六维 counters/period 仍由实际 SyncRegions 字段快照 检验，
没有为匹配 Rust 删除它们。PdClient 解码真实 Go 响应，去 header 后以 prost 规范化编码保存，**不是原始 Go 序列化字节**。
cluster identity/header 属于已由任务50验收的 transport
边界，本主题捕获的帧去 header 后比较整个领域状态，不能称为新增 header wire 验收。

固定顺序：region1(epoch2/2,term7) → duplicate → window → bucket v3 → bucket old v2 →
bucket duplicate v3 → 三次 store/read window → stale version/confver/term → term8
（关闭 bucket inheritance，丢弃旧独立 report，history 前进）→ split right region2 →
split left region1 → merge region1(epoch4/3,term9)。每步等待稳定点后读取所有状态。
written/read counter 单位为报告累计 bytes/keys；approximate size 为 MiB；bucket
period 为毫秒、HTTP bucket 为每秒 rate；history 依次 1、1…1、2、3、4、5。
持续下游初始全量 completion=1；重复/bucket/store/stale 没有 live 帧；term/split right
分别收到 index1/2；此时关闭流，left/merge 期间离线，从 index3 重连取得两条记录至5。
每步还开独立全量流，snapshot offset=0，completion 使用真实 history index。

[重启 fixture](../../../tests/rust-compat/fixtures/state/task-53-restart.json)：再发送
95 条 epoch5..99 的实际心跳并完整消费请求/响应流至 EOF，history=100；TERM 并 wait
原 Go 进程，保留原 WAL/LevelDB/Etcd 数据，以同参数启动新进程，full completion=100，
完整 KV metadata 不变、源端 leader/flow 按 storage 重建归零；再发送 epoch100 后为101。
最后 Go 进程 TERM/wait exit0，临时数据删除。EOF 的 Unknown/EOF 返回原样记录，
只作为既有 Go heartbeat 输入耗尽的探针生命周期结果，不冒称新网络 EOF 协议等价。

归一化限于无序 region/hot 集合及其对齐附属数组、JSON 整数/整数浮点词法、宿主时钟
原点（StoreMeta.LastHeartbeat/read更新时间）；2^53 外整数不经 f64 改写，无数值容差。
所有 epoch、history、counter、bucket 版本/周期保留。write更新时间由实际 interval=60
秒验证并转换，已被覆盖 region 的零时间不替换。
Go stale 原始消息留在 fixture；领域稳定错误比较 error type + incoming/current 完整
metadata，current 从同一实际 Go sync 帧读取，不从 Rust 结果生成期望。
不把 Rust Debug 与 gogo protobuf 文本表示当作相同格式，也不声称做了错误散文的 wire 比较。

## 红灯、实现与真实缺口修复

[red.out](task53/red.out) 实际运行3项并失败：故意错误期望副本 epoch.version=999
报告 `$/regions/regions/0/epoch/version: 999 != 2`。已移出真实回放输入，保留独立
负控制断言验证差异检测；没有修改生产实现制造红灯。此前 example 缺 region_epoch
字段的编译错误只作为接口修正，不计行为失败证据。

修正宿主 bootstrap 的 rolling entry/trace 配置后，[focus-1.out](task53/focus-1.out)
1通过/2失败，两后端同在 bucket-v3 的完整 sync bucket bytes 断言失败：Go 包含六组
原始计数和 period2000，Rust 只有 region/version/keys。这是联合运行差分发现的真实缺口。

- core `RegionBucketStats`/`RegionBuckets` 增加 owned raw stats/period，无 protobuf 依赖。
- cluster `handle_bucket_heartbeat` 保存完整 report；低/同版本仍保留旧报告的全部字段，
  但向真实 worker 提交当前报告，热点度继续增长，与 Go 一致。
- server `records_to_proto` 在 full/history/catch-up 转发完整原始 report；heartbeat
  BucketMeta 优先时 stats=None/period0。follower 继续按 Go SetBuckets 只消费 metadata。
- 原任务52测试改为比较 follower 消费的 region_id/version/keys；source完整报告由本次
  Go 运行字段快照验证，不再用 source raw report 与 follower BucketMeta 的结构体全等。
- 新 cfg(test) state_regression 模块仅装配 canonical 组件，不复制统计、规则或索引算法。
  production source 只有上述最小 raw-report 修复；依赖新增均为 compat dev dependency，
  Cargo.lock 仅10条既有 workspace 引用，无新版本或猜测 feature。

同一16步+95追加事件在 MemoryStore/真实 EtcdMetaStore 重放；初始/增量/重连所有帧、
整个稳定点快照一致。另将源唯一 Downstreams 已接受的100条真实记录送入 canonical
PersistentHistory 验证自动100次 checkpoint；这两个实例是验证内存历史与持久包装器
等价，不是生产新增第二份历史。停止源 workers，Etcd 用同 WAL stop/start，重新加载
KV metadata/空 replay ring100，下一事件两种 canonical history 同为101，源帧与 Go 相同。

包级首次失败 [first-2.out](task53/first-2.out)：多个 `#[path]` 引用的 fixture 各自拥有
NEXT=0，使用同进程同名目录，产生真实 WAL 创建冲突；已统一到 harness 的唯一 fixture
模块，原 schedule_foundation 测试也复用它。没有新增 Go 工具、全局 runtime 或业务结果 mock。

## 来源覆盖与证据分类

| 来源要求 | Go 实际运行证据 / 当前 Rust 验证 |
| --- | --- |
| 乱序、重复、epoch/confver/term | 本次16步联合快照；[任务43](task-43.md) 的 stale/overlap 原测试与本次 workspace 回归 |
| split/merge、KV 覆盖清理 | 本次每步 root/KV/full/live/catch-up；[任务44](task-44.md)、[任务46](task-46.md) |
| bucket 版本竞争 | 本次 v3/v2/v3 全字段快照；[任务45](task-45.md) 的真实 CAS 调试 concurrent/retry 输出及相应 Rust 测试 |
| store removed | [任务42](task-42.md)、[任务45](task-45.md) tombstone 运行输出、[任务47](task-47.md) 协议运行输出；相应测试全量回归 |
| 统计窗口/阈值 | 本次三次 write/store read 报告及全部热点值；[任务18](task-18.md)、[任务20](task-20.md)、[任务23](task-23.md) 的原 Go 运行断言与 Rust 窗口测试 |
| 初始同步、增量、断连追赶 | 本次35个主状态帧；[任务34](task-34.md) 至 [任务38](task-38.md) 的1001条/追赶/ready 原 Go 运行证据；[任务51](task-51.md) 网络与本次 server 回归 |
| history 溢出 | 新主题测试直接读取 [真实 Go ring fixture](../../../tests/rust-compat/fixtures/state/task-30.json)，reset100、输入1/2/3、cap2、first101/next103、保留2/3；[任务35](task-35.md) 的原Go超限失败输出 |
| watch revision/compaction | [本轮 Go 六子测试输出](task53/dependency-watch.out)、[任务39](task-39.md)；本次真实两后端分页/增量/compaction对账，保留 entry.mod_revision 与 snapshot revision 的区别 |
| 持久索引重启 | 本次真实 Go 100→100→101、同事件 Rust/Etcd 重启；[任务32](task-32.md) 已有六条/101条持久化原 Go 断言 |
| 取消/join | 本次源、下游、runners/HotStat 全部关闭；真实 watch pending 后取消/join，取消具有终态；阻塞 Send 进入后取消且不确认 index，join 后通道关闭；[任务40](task-40.md) 四项实际 Go 生命周期输出 |

源码推导与故障验证分开：六条历史记录加 cap2、MetadataWatch 的三键→两键更新→删除
并 compact→新键接续，以及 blocked sender 的 future-drop 是 Rust 领域故障扩展，
复用上述已运行 Go 回调/索引/取消契约，未冒称新增 Go 联合故障 JSON。Go 重启则由本次
实际完整服务输出补足，未用“环境问题”放行必要 oracle。Rust watch 单独保持 revision，
history 写入不推进 watch cursor；真实停服时 Unavailable 保留视图/cursor，恢复后接续。
所有成功路径读取实际 backend/caches；只在发送阻塞、取消和真实服务 I/O 断开边界控制。

## 最终验证、资源及只读审计

入口 `bash docs/rust/state/task53/slot.sh python3 docs/rust/state/task53/validate.py`。
精确 argv/退出码/有效计数见 [summary.json](task53/summary.json)，
[阶段摘要](task53/validation.out) 与 validation-0.out 至 validation-7.out 保存原始输出。
本任务所有阶段动态取得槽位 **2**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
锁 owner 记录 plan/task/session/PID，所有阶段先结束并 wait 自己的子进程才释放锁，缓存保留。

| 最终命令 | 退出码 / 有效结果 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo test -p astersql-pd-syncutil --locked --lib ordered_single_flight_test::singleflight_randomized -- --nocapture` | 0 / 1通过 |
| `cargo test -p astersql-pd-compat-tests --locked --lib state_theme_ -- --nocapture` | 0 / 6通过、0失败/忽略 |
| `cargo test -p astersql-pd-compat-tests --locked` | 0 / 20通过、0失败、1既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0 |
| `cargo test --workspace --locked` | 0 / 1312通过、0失败、1既有忽略 |

既有 ignored 为需要独立进程二进制的 tso_processes，本任务聚焦无忽略。
上一次 workspace 曾在未修改的 syncutil 随机测试触发 poll-is-pending 断言，完整失败
保存在 [second-validation-6.out](task53/second-validation-6.out)；未修改或屏蔽该测试，
单独重跑及最终 workspace 均通过，不将当次失败计为通过。

[audit.py](task53/audit.py)、[readonly.json](task53/readonly.json) 核对起终点1101项
Go/模块清单、逐项 SHA256/Git 状态及29份 plan 哈希完全相同；聚合SHA256：
`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
[resources.json](task53/resources.json) 保存锁释放、缓存保留、自有进程退出和失败夹具
WAL清理记录。其他槽位的既有锁/进程未操作。源码/模块无新增 Go 文件或内容变化，
保留用户既有计划改动；格式化仅本任务触及的 Rust 文件，Markdown 链接与 diff 检查通过。

既有 etcd_fixture_test.rs → scripts/rust-etcd-fixture/main.go 调用链已审查，仅 go build
现存夹具到所持槽位，无测试生成/注入；EOF请求停止、必要时kill并wait。proto/build.rs
读取固定模块ZIP，仅写OUT_DIR。所有子进程继承
`CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off GOARCH=arm64`。
未运行会改写Go的make、注入、生成、格式化或模块整理入口。库使用宿主Handle，
只有独立oracle探针二进制创建自己的host runtime，不给library安装全局runtime/信号/日志。

状态：已完成。交付状态输入面及同步适配的联合回归，不声明整个PD服务或调度决策迁移完成。

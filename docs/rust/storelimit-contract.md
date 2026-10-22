# Storelimit 行为覆盖契约

盘点日期：2026-10-22。核心领域模型计划任务 1，批次 1，无依赖批次。
Go 基准为 `0820f0b67b0e49b289b84fdf66b7b8fb178afe82`，本次审计 HEAD 为
`eb85dc23c66fb15948fdfe539bca99d9f2d1ec9f`；两者之间本包无差异。
本文固定后续任务的验收责任，不宣称 Rust storelimit 已实现。
架构遵循 [ADR-001](architecture.md)、[移植指南](porting-guide.md) 和
[依赖基线](dependencies.md)。编号指核心领域模型计划中的任务编号，后续删除任务文件
不会改变编号含义，也不能代替实现与验证证据。

## 源码与验收责任

三个生产文件为 [limit.go](../../pkg/core/storelimit/limit.go)、
[store_limit.go](../../pkg/core/storelimit/store_limit.go) 和
[sliding_window.go](../../pkg/core/storelimit/sliding_window.go)。
下表覆盖类型、常量、变量、全部函数及接口方法，包括内部行为。
Go 用例均指 [limit_test.go](../../pkg/core/storelimit/limit_test.go)；“待补”表示已有
用例未直接验证该项，必须由负责任务的共享 fixture/oracle 补充。
调用方代号见下一节。当前本包没有 Rust 文件或 Cargo manifest，因此表内所有
storelimit 行为的 Rust 覆盖均为 **缺失**；已有叶子 API 的证据另列，不算本包覆盖。

| Go 源码及符号 | 可见性、契约与用途 | 调用方 | Go 测试与缺口 | 负责任务 |
| --- | --- | --- | --- | --- |
| `limit.go`: `Type`, `AddPeer`, `RemovePeer`, `SendSnapshot`, `TransferLeaderIn` | 公开；数值依次 0、1、2、3 | C1–C8 | 现有测试使用部分枚举；完整数值与名称待补 | 2 类型映射 |
| `limit.go`: `storeLimitTypeLen` | 内部；值 4，分配独立类型桶 | `NewStoreRateLimit` | 四桶隔离待补 | 2、3 |
| `limit.go`: `VersionV1`, `VersionV2` | 公开；字符串 `v1`、`v2` | C1、C5、C7 | 本包无版本断言；C5 cluster 测试有版本切换 | 2 定义，3/6 返回，19 装配 |
| `limit.go`: `StoreLimit` | 公开接口，包含 `Available`, `Take`, `Reset`, `Feedback`, `Ack`, `Version` 六方法；查询、消费、完成返还的 cost 须一致 | C1、C3、C5 | 两实现均编译期断言符合接口；跨实现装配待补 | 3/4 v1，6/7 v2，19 统一装配 |
| `store_limit.go`: `SmallRegionThreshold` | 公开；20，小 region 边界由调用方判断 | C3 | 本包不测分类；operator influence 测试及任务 2 常量 oracle | 2；调度分类范围外 |
| `store_limit.go`: `Unlimited` | 公开；1e8，有限大数标志，非 `f64::MAX` | C3、C5–C8 | 1e8 上下边界待补 | 2 定义，4 重置行为 |
| `store_limit.go`: `influence`, `smallInfluence` | 内部；1000、200 | 影响表、`limit.Reset` | 完整常量待补 | 2、4 |
| `store_limit.go`: `RegionInfluence` | 公开切片，四项均 1000 | C1、C3 | 完整表待补 | 2 |
| `store_limit.go`: `SmallRegionInfluence` | 公开切片，仅 AddPeer/RemovePeer 两项为 200；其余索引越界，不能伪造四项 | C3 | 长度、索引适用范围待补 | 2 |
| `store_limit.go`: `TypeNameValue`, `Type.String` | 公开；四名称 `add-peer`、`remove-peer`、`send-snapshot`、`transfer-leader-in`；未知枚举 String 返回空串，未知名称 map lookup 的 ok 为 false；不依赖 map 遍历顺序 | C3、C5–C7 | 全名称、未知输入待补 | 2 |
| `store_limit.go`: `StoreRateLimit`, `NewStoreRateLimit` | 公开；四个独立 `limit`，逐个以相同 rate Reset，返回接口 | C1、C5 | `TestStoreLimit`，四桶隔离及零初值待补 | 3，零 rate 由 4 完整验证 |
| `store_limit.go`: `StoreRateLimit.Available`, `StoreRateLimit.Take` | 公开；SendSnapshot 直接 true，其余按 Type 选桶；忽略优先级；Available 查询，Take 消耗并在不足时拒绝 | C1–C3 | `TestStoreLimit`；snapshot 旁路、多类型及优先级待补 | 3 |
| `store_limit.go`: `StoreRateLimit.Rate` | 公开；返回原始 rate；槽位 nil 返回 0；不作 Type 越界防护 | C3、C5 | `TestStoreLimit` 仅初始 rate，nil 槽位待补 | 3、4 |
| `store_limit.go`: `StoreRateLimit.Reset` | 公开；snapshot 不处理，其他桶重置 | C1、C3、C5 | `TestStoreLimit` 同 rate 与零 rate；snapshot 旁路待补 | 4 |
| `store_limit.go`: `StoreRateLimit.Ack`, `StoreRateLimit.Feedback`, `StoreRateLimit.Version` | 公开；前两者无操作，版本 v1 | C1、C3、C5 | 无操作及版本的直接断言待补 | 3 |
| `store_limit.go`: `limit` | 内部；limiter 指针、读写锁、原始 rate；初始 rate 为 0 可使首次 Reset(0) 不建立桶 | `StoreRateLimit` | `TestStoreLimit`；新建零 rate 待补 | 3、4 |
| `store_limit.go`: `limit.Reset` | 内部；相同 rate 不变更桶；rate>=1e8 时容量 1e8 且补充速率保留输入；否则容量为 1000 或 rate>1 时截断 rate*1000，补充速率为 rate*1000；改变 rate 新建满桶 | 构造器、`StoreRateLimit.Reset` | `TestStoreLimit`；小数、1、Unlimited、换 rate 及补充待补 | 4 |
| `store_limit.go`: `limit.Available`, `limit.Take`, `limit.GetRatePerSec` | 内部；读锁保护；原始 rate==0 时前两者直接 true，否则委托 `RateLimiter.Available`/`AllowN`；Get 返回原始 rate | 同名公开封装、`Rate` | `TestStoreLimit`；有符号 cost 与时间边界待补 | 3、4 |
| `sliding_window.go`: `minSnapSize`, `defaultWindowSize` | 内部；10、100，最小剩余空间判断及初始容量 | 窗口、构造、反馈 | `TestWindow`, `TestSlidingWindow` | 5、6 |
| `sliding_window.go`: `defaultProportion`, `defaultIntegral` | 内部；20、10，反馈比例与积分系数 | `Feedback` | `TestFeedback` 仅最终区间，确定误差序列待补 | 7 |
| `sliding_window.go`: `SlidingWindows`, `NewSlidingWindows` | 公开；四级窗口，初始容量依次 100、50、25、12，读写锁保护；lastSum 初值 0 | C5；C1 保存接口 | `TestSlidingWindow` 断言构造容量 | 6 |
| `sliding_window.go`: `SlidingWindows.Version`, `SlidingWindows.Reset` | 公开；版本 v2；Reset 对任何 rate/type 均无操作 | C1、C5 | 直接断言待补 | 6 |
| `sliding_window.go`: `SlidingWindows.GetCap`, `SlidingWindows.GetUsed` | 公开；返回第 0 窗口容量和独立 used 切片快照，读锁保护 | C4 | `TestSlidingWindow`, `TestFeedback`；快照隔离待补 | 6、7 |
| `sliding_window.go`: `SlidingWindows.Available` | 公开；snapshot 扫描 0..=level，任一窗口可用即 true；忽略 cost；非 snapshot 恒 true | C1、C2、C3 | `TestSlidingWindow`；四级及门槛待补 | 6 |
| `sliding_window.go`: `SlidingWindows.Take` | 公开；snapshot 从低到高选择首个可用窗口，消费整个 token；非 snapshot 恒 true 且不改变 used | C3 | `TestSlidingWindow`；高、urgent 耗尽路径待补 | 6 |
| `sliding_window.go`: `SlidingWindows.Ack` | 公开；snapshot 从高到低返还，将余数传至下层；非 snapshot 无操作 | C3 | `TestSlidingWindow`；跨四级和过量返还待补 | 6 |
| `sliding_window.go`: `SlidingWindows.Feedback` | 公开；low 尚可用则完全不变；否则 lastSum+=e，cap=max(100,20*e+10*lastSum)，再 set(snapshot) | C1、C5 | `TestFeedback` 使用随机大小和 ticker；确定序列与截断待补 | 7 |
| `sliding_window.go`: `SlidingWindows.set` | 内部；仅 snapshot；负 cap 先改为 10；各层容量按 int64(cap)>>i，再由 reset 保底；不清 used | `Feedback`；`TestSlidingWindow` | `TestSlidingWindow`；负值、截断及保留 used 待补 | 6 基础设置，7 反馈调容 |
| `sliding_window.go`: `window`, `newWindow`, `window.reset` | 内部；capacity 至少 10；used 初值 0，count 为 uint 初值 0；reset 仅改容量 | `NewSlidingWindows`、`set` | `TestWindow`；reset 保留 used/count 待补 | 5 |
| `sliding_window.go`: `window.available`, `window.take`, `window.getUsed` | 内部；used+10<=capacity 即可接纳，take 在判断后增加整个 token 和 count，允许一次越容量；getUsed 返回 used | 多级窗口 | `TestWindow`, `TestSlidingWindow` | 5 |
| `sliding_window.go`: `window.ack` | 内部；used==0 时直接返回 token；否则扣到零并返回剩余 token，每次非空 ack 将 count 减一；并非按 cost 预留的严格配对模型 | `SlidingWindows.Ack` | `TestWindow` 覆盖空、恰好、超量；部分扣减待补 | 5 |

Go 的 Type/priority 不是封闭枚举。v1 非 snapshot 的无效 Type 会触发索引越界；
v2 snapshot 的负 level 会跳过循环返回 false，而超出窗口范围的 level 只有执行到越界
索引才 panic。非 snapshot 在访问 level 前旁路。任务 2/3/6 需记录 Rust 的对应边界，
不得自行把所有未知值当作默认类型。负 token、非有限 rate/反馈及整数溢出没有专门的
Go 测试，不能在本次文档审计中宣称兼容；后续相关任务须用真实 Go oracle 固定所接受的
输入域和精度。count 是内部状态，不是可用于替代 used 的可用性依据。

## 调用方清单与范围

这里列出全部直接使用 `storelimit.` 的生产文件，并补充通过接口调用的路径。

| 代号 | 源码路径与调用 | 迁移责任 |
| --- | --- | --- |
| C1 | [core/store.go](../../pkg/core/store.go)：默认 v1、limiter 字段、Clone/ShallowClone 共享、LimitVersion、Feedback、IsAvailable、GetStoreLimit、StoresInfo.ResetStoreLimit；[store_option.go](../../pkg/core/store_option.go)：ResetStoreLimit、SetStoreLimit | 16 共享；19 装配；27 按 ID 控制 |
| C2 | [filter/filters.go](../../pkg/schedule/filter/filters.go) 与 [region_filters.go](../../pkg/schedule/filter/region_filters.go)：IsAvailable 检查 AddPeer/RemovePeer/TransferLeaderIn/SendSnapshot | 本计划提供 19 的可用性 API；完整 filter/调度迁移范围外 |
| C3 | [operator/influence.go](../../pkg/schedule/operator/influence.go)：StepCost 与大小影响；[step.go](../../pkg/schedule/operator/step.go)：peer/snapshot/leader cost；[operator_controller.go](../../pkg/schedule/operator/operator_controller.go)：检查、Take、Ack、按配置 rate/60 重置、Rate 类型断言 | 2–7 提供行为；完整 operator controller 及调度策略范围外，不把外部 rate/60 换算移入限流库 |
| C4 | [statistics/store_collection.go](../../pkg/statistics/store_collection.go)：断言 SlidingWindows，读取 GetCap/GetUsed，上报 `windows_size` 与 `windows_used_level_` 指标 | 6/7 保留观察能力；statistics 指标采集迁移范围外 |
| C5 | [server/cluster/cluster.go](../../server/cluster/cluster.go)：按版本构造、更换 limiter；snapshot 反馈；Rate 查询及重置；[server/handler.go](../../server/handler.go)：设置 Store limit | 19 覆盖领域装配；心跳服务及配置网络接入范围外 |
| C6 | [schedule/config/config.go](../../pkg/schedule/config/config.go)、[config_provider.go](../../pkg/schedule/config/config_provider.go)、[server/config/persist_options.go](../../server/config/persist_options.go)、[mcs scheduling config](../../pkg/mcs/scheduling/server/config/config.go)：默认值、按类型读写，含 TransferLeaderIn、Unlimited | 2 保留枚举和常量；配置、持久化和 MCS 迁移范围外 |
| C7 | [server/api/store.go](../../server/api/store.go)：v1 校验、名称解析；[pd-ctl store_command.go](../../tools/pd-ctl/pdctl/command/store_command.go)：Unlimited 参数边界 | 2 提供名称查找；HTTP/CLI 校验与工具迁移范围外 |
| C8 | [mockcluster.go](../../pkg/mock/mockcluster/mockcluster.go)：测试集群默认 AddPeer/RemovePeer limit | Go 测试辅助，非生产 Rust API |

直接引用的测试文件可用 `rg -l 'storelimit\.' --glob '*_test.go'` 重建，分布在
`pkg/core`、`pkg/statistics`、`pkg/schedule/{checker,filter,operator,scatter,schedulers}`、
`pkg/mcs/scheduling/server/config`、`server/{api,cluster,config}`、`server/server_test.go`、
`tests/server/{api,cluster}`、`tests/integrations/mcs/scheduling` 及 `tools/pd-ctl/tests/store`。
这些是调用方证据，不是本次执行的测试。包内四个行为测试为 TestStoreLimit、
TestSlidingWindow、TestWindow、TestFeedback；TestMain 只负责 goleak。

本包没有工厂对象池、Runner、指标注册、生成代码或 failpoint 调用。
两个公开构造器属于任务 3/6；版本选择属于任务 19。全局 Runner、Go 工厂池机械翻译、
statistics 指标移植均在范围外；任务 59 仅在 server 做 heartbeat 阶段追踪并接收宿主
Registry，不能把它当作 storelimit 指标已迁移。任务 60/61 处理 Store/Region 的协议
转换，不允许 storelimit 引入 proto 来弥补领域 API。

## 已有 Rust 依赖的实际证据

已读根 [Cargo.toml](../../Cargo.toml)、[constant manifest](../../pkg/core/constant/Cargo.toml)、
[ratelimit manifest](../../pkg/ratelimit/Cargo.toml) 及其真正 lib 入口。

| 依赖 | 当前源码 API 和已有测试 | 差异、缺口及后续责任 |
| --- | --- | --- |
| `astersql-pd-core-constant` | lib 入口是 [kind.rs](../../pkg/core/constant/kind.rs)，并非 lib.rs；`PriorityLevel::{LOW,MEDIUM,HIGH,URGENT,LEN}` 数值 0..4，`from_i32`、`as_i32`、Display；[kind_test.rs](../../pkg/core/constant/kind_test.rs) 的 `priority_and_resource_kind` 覆盖四级、长度与未知显示 | 可复用；不存在 `PriorityLevelLen` 或直接整数索引 API；任务 6 显式处理数值转换及无效值。注释的优先级措辞不能覆盖实际枚举顺序/Go 低到高扫描语义 |
| `astersql-pd-ratelimit` | [lib.rs](../../pkg/ratelimit/lib.rs) 重导出 [rate_limiter.rs](../../pkg/ratelimit/rate_limiter.rs) 的 `RateLimiter::new(f64,i64)`、async `available(i64)`、`allow_n(i64)`、`set_limit`、`set_burst`、`limit`、`burst`，另有 allow/wait_n；初始满桶，Tokio Instant 计时 | storelimit 所需查询/消费/补充能力存在，但没有同步 `Available/AllowN`，也没有注入时钟构造器。任务 3/19 必须显式传播 async 或在其故事内解决接口边界；禁止 block_on/新建私有运行时。任务 4 可沿用暂停 Tokio 时钟测试，不得写成已有显式时钟参数 |
| ratelimit 测试 | [rate_limiter_test.rs](../../pkg/ratelimit/rate_limiter_test.rs) 注册于 lib.rs：`token_bucket_oracle_and_updates`、`token_bucket_wait_cancel_deadline_and_refund`、`token_bucket_zero_infinite_and_concurrency`、`token_bucket_wait_serializes_and_drop_refunds`、`token_bucket_fractional_refill_and_burst_growth`、`token_bucket_available_does_not_consume_on_running_clock` 六项 | 源码覆盖查询不消费、耗尽、补充、小数、零/无限和并发；本次未运行。裸桶零 rate 会耗尽，与 storelimit 的零 rate 旁路不同；INF=f64::MAX 与 Unlimited=1e8 不同。任务 3/4 必须自行验证封装语义 |
| storelimit crate | 当前无 manifest、lib 或实现；workspace 中也无 `astersql-pd-storelimit` | 任务 2 登记 canonical crate；3–7 逐项实现。此项是计划交付，不是声称已有的叶子依赖 |

历史溯源：`2c3c729d3` 引入 Rust token bucket，`94376cb47` 做叶子 API 与回归；
本次直接检查当前源码、测试注册及 `git log`，不把历史提交标题当作本次运行成功证据。
未发现阻止文档审计的必需依赖缺失；async/sync 差异和未提供的显式时钟构造器已如实
记录。后续若要求纯同步领域接口，任务 3 须先验证可行接口边界再实施，不能凭 crate
存在就认定集成完成。movingaverage、rangetree 和 proto 不是这三个生产文件的依赖。

## 后续验证前置条件与本次证据

任务 2–7 各自使用同一份 JSON fixture 驱动真实 Go 和 Rust，新增 Go oracle 后再实现；
缺失测试不能从本表的源码推导冒充 Go 执行结果。任务 4 的时间补充用可控时刻，任务 7
用确定误差序列替代对随机 ticker 终值的单独依赖。Rust 的浮点转整数、右移和边界要
逐项同 oracle 比较；不忽略错误结果。69 最终核对基准后漂移和传递依赖，68 逐项填入
真实 Rust 符号、红绿差分与回归证据。

后续代码任务执行 Go oracle 前须具备仓库要求的 Go 工具链、依赖及 failpoint 工具；
用 make gotest 并保证注入禁用后才执行其他操作。Cargo 阶段须领取共享槽位，准备已锁定
依赖缓存/网络，并使用宿主 Tokio 测试运行时；本任务不领取槽位、不执行 Cargo/Go 测试。

本次文档检查命令（仓库根执行）：

```sh
rg -n '^func |^type |failpoint\.' pkg/core/storelimit -g '*.go'
rg -n 'storelimit\.' --glob '*.go' --glob '!**/*_test.go'
rg -n 'GetStoreLimit\(|\.Feedback\(|\.Ack\(|\.GetCap\(|\.GetUsed\(' pkg/core pkg/schedule pkg/statistics server --glob '*.go'
git diff 0820f0b67b0e49b289b84fdf66b7b8fb178afe82 -- pkg/core/storelimit
git diff --check
```

以上命令退出码均为 0；基准差异为空。另以本地路径检查逐一验证本文 Markdown 链接、
反引号内现存文件路径、全部生产声明覆盖、接口方法和直接生产调用方覆盖；均通过。
具体数量：32 个 Markdown 链接、3 个生产文件的 53 项声明、6 个接口方法、17 个直接
生产调用文件（含测试辅助 mockcluster）；路径与覆盖检查脚本退出码为 0。
本次有效执行行为测试数为 0（文档任务不适用），不以此宣称生产行为迁移通过。

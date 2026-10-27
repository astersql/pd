# 任务154：范围均衡配置传输适配

状态：已完成。

前置153提交 `bf178fdd0` 的七条必需命令均退出0，日志哈希已重新核实。
本任务复用已验收的 `BalanceRangeJob`、`BalanceRangeJobs`、`BalanceRangeScheduler`；
没有新增 crate 或 feature，仅将已有 workspace keyutil/typeutil 声明为 server 的直接
依赖。Cargo.lock 只有该路径包的两条依赖变化，没有远程版本变化。

## 来源与契约

按用户明确授权，采用只读 Go 源码/测试推导加 Rust 真实行为和故障验证，替代缺失的
Go 运行 oracle/运行差分准入。**没有执行 Go 测试，没有声称 Go 运行差分**；
不修改 oracle-index 的既有证据分类，也不修改只读 plan 或原主题。
[provenance](logs/task_154/provenance.json)记录来源 commit、各 Go 文件与 fixture
SHA-256、外部只读 pingcap/errors JSON 编码源码和前置153证据；来源均与 commit 相同。

Go balanceRangeScheduler 明确覆盖 ServeHTTP，不能套用 BaseScheduler。
POST `/config` 不读 body，返回400 JSON字符串；GET `/list` 返回 clone 后的非null数组；
PUT `/job` 新增并保存，DELETE `/job?job-id=` 取消并保存，成功都是200 `null`。
新增验证/保存失败400字符串，取消不存在/已完成/I/O失败作业均500 `{}`。
固定版本errors的FastGenByArgs会SuspendStack包装，withStack不暴露JSON字段，不能
直接按内部Error的MarshalJSON渲染typed error。全部JSON保留render追加的末尾换行。方法错误405空body、无Allow；未知路径404明文及X-Content-Type-Options:nosniff。

已逐项核实指定 Go 测试的实际范围：

- `TestPlacementRule`：leader/peer/learner 规则与可用节点阈值，不覆盖 HTTP。
- `TestIsBalanced`：finished/running/cancelled 及范围清理，不覆盖 HTTP。
- `TestPrepareBalanceRange`：范围内 leader 计数和期望分数，不覆盖 HTTP。
- `TestTIKVEngine`：真实转移/搬迁及限速，不覆盖 HTTP 方法/响应。
- `TestLocationLabel`：拓扑规则、平衡及不健康节点过滤，不覆盖 HTTP。
- 相邻 `TestAddBalanceRangeJobWithInvalidFieldType` 覆盖部分 PUT handler 参数类型；
  `TestPersistFail` 用 persistFail failpoint 验证配置回滚。本任务没有裸跑这些测试，
  不用列出名字代替运行证据。

[冻结 fixture](fixtures/task_154.json)包含109条源码推导HTTP案例，覆盖四个方法/路径、
query、405/404、无效字段类型/缺失值、引擎/规则、duration 正负/溢出/默认/小数、
范围数量和两次URL解码、空/多/二进制范围、job-id 数字边界和符号、JSON类型/EOF、
未知/重复字段及六种 engine/rule 组合。
status/header、字符串/成功响应字节、内存及持久化字段不归一化；结构化错误和列表
比较全部字段，仅忽略 JSON key 顺序/空白。stack 只检查非空结构；时钟固定 UTC，
没有动态时间字段归一化。任意损坏 JSON 诊断和 Gorilla 路径清理重定向不在固定
案例兼容声明内；完整服务路由挂载及线上HEAD body抑制仍归后续主题。

## 实现与失败—通过证据

生产路径 [schedule_admin_balance_range.rs](../../../pkg/server/schedule_admin_balance_range.rs)
公开 `serve(&mut BalanceRangeJobs, JobTime, Request<Body>)`，测试在 server/lib.rs 显式注册。
[说明](../../../pkg/server/schedule_admin_balance_range.rs.md)记录边界、错误和存储语义。

先注册冻结fixture的真实行为测试，以错误501响应执行[红测](logs/task_154/red.json)：
聚焦命令退出101，0通过/1失败，具体断言为501与400不符，能够检出错误行为。
实现后[第一次验证](logs/task_154/green.json)发现 canonical keyutil 的缺参数错误文本
为 `%v` 而 Go HTTP 是 `%!v(MISSING)`；在传输层按来源修正，没有改领域实现。
[green2](logs/task_154/green2.json)为3通过，增加调度测试时
[green3](logs/task_154/green3.json)记录缺少 trait 导入造成的编译失败，改用 canonical
regions.get_region 后 [green4](logs/task_154/green4.json)为4通过；这些中间结果全部保留。
扩大fixture至109条后进行了首次全量验证。源码复核进一步发现固定render在
IndentJSON后追加换行，以及errors.SuspendStack会包裹内部typed error（取消错误实际
为`{}`）；保留[修正前fixture](logs/task_154/pre-source-review-fixture.json.gz)和
[修正前全量结果](logs/task_154/pre-review-verified.json)，它们不作为最终兼容证据。
更正源码推导期望后，[source-review-red](logs/task_154/source-review-red.json)为
1通过/3失败，检出缺少响应换行；只修正换行后
[wrapper-red](logs/task_154/wrapper-red.json)检出误用typed error响应。
修正包装响应后的[一轮全量结果](logs/task_154/post-review-verified.json)中，
六条命令退出0，workspace测试在未修改的syncutil::singleflight_randomized的
register(poll)断言偶发失败（退出101）；前次全仓运行曾通过，没有修改该无关测试。
继续源码审查补齐默认http.NotFound的nosniff响应头，
[header-red](logs/task_154/header-red.json)为3通过/1失败，明确检出缺失响应头。
补齐后重跑下表全部七条必需验证，均退出0，singleflight_randomized也通过。

四个真实测试：

- `task_154_behavior_cases`：109条HTTP案例；逐案确认jobs、默认/指定纳秒timeout、
  engine/rule、固定create、pending、真实MemoryStore持久化内容及无效请求无写入。
- `task_154_job_lifecycle_and_list`：新增/list、活动alias重复、query首值/前导零、取消
  起止时间、已完成包装错误对象、alias复用和递增ID、保留running原start、reload重建。
- `task_154_persistence_and_body_faults`：新增与取消 I/O 失败保留内存/持久快照；
  五条不读body路由用poll即panic流验证，两个body读取失败含完整合法JSON前缀均
  500/internal且不应用；两个提交前pending I/O被取消后保留快照；恢复后重试与reload。
- `task_154_http_job_drives_real_schedule`：两种conf-change协议形式，真实core、统计、
  placement、存储、范围管理和operator。空队列无调度；HTTP新增后限额拦截仍pending，
  放行后running并登记范围；六个leader regions产生单步 `Range|Leader`、Medium
  TransferLeader（region1，1→3），核对分数6/0、期望2、kind、优先级、目标、影响量、
  confver=0、心跳命令及应用后完成。HTTP取消保存失败保留running/范围，成功后
  check_job清理范围和报告、停止调度，重新加载持久cancelled。

调度固定时钟10^12ns、随机索引0、不打乱、ID排序下目标3。该目标是Rust受控顺序的
确定结果，Go相同分数候选2/3的遍历顺序未声称运行一致。未忽略错误、步骤或影响量。
所有领域对象真实，仅时间、随机与MetaStore外部I/O边界可控。

## 验证、槽位与只读审计

[verify.py](logs/task_154/verify.py)原子mkdir按1–10动态领取仓库共享槽位，写owner记录，
所有Cargo、脚本及子进程继承绝对CARGO_TARGET_DIR。各阶段实际均领取 **槽位1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
默认features；CARGO_BUILD_JOBS=4、RUST_TEST_THREADS=2。失败/正常/中断等待子进程
结束后释放自己的锁，保留编译缓存。

[只读Go拦截器](logs/task_154/reuse-go/go)拒绝实际Go执行，仅核验已有kvproto ZIP并
复用已有etcd fixture二进制；[记录](logs/task_154/go-reuse.jsonl)。没有make、failpoint
注入/启停、Go构建/测试、模块整理或Go生成。该二进制只供既有Rust测试的etcd外部
I/O边界使用，不是HTTP契约的Go运行oracle。

最终[确切命令、退出码和完整日志](logs/task_154/verified.json)及
[有效测试数量](logs/task_154/verified-summary.json)：

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务Rust格式变化 |
| `cargo test -p astersql-pd-server --locked --lib task_154 -- --nocapture` | 0 | 4通过、0失败、0忽略；109条HTTP案例 |
| `cargo test -p astersql-pd-server --locked` | 0 | 118通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1918通过、0失败、1既有忽略（tso_processes） |

[审计脚本](logs/task_154/audit.py)和[结果](logs/task_154/audit.json)对照
[开始快照](logs/task_154/baseline.json)核对全部Go/模块文件集合、内容哈希与Git状态
（含untracked）、两份只读plan，共1103个文件；内容与状态不变。
日志/provenance哈希、Markdown链接和git diff --check均通过。最终自有槽位锁已释放，
保留缓存。仅提交本任务实现、测试、接线及证据，排除其它任务既有删除；完成后删除
154编号任务文件，不修改plan或原主题。

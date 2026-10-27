# 任务153：慢趋势驱逐配置传输适配

状态：已完成。

前置152提交 `2ffefa04a` 的七项必需命令退出0，保存日志哈希及当前API已核实。
本任务复用canonical `SlowTrendScheduler::update_recovery`、`observe`、`schedule`，
没有新增crate、feature、依赖或更改Cargo锁。

## 来源与契约

按用户明确授权，使用只读Go源码/测试推导加Rust真实行为和故障验证替代缺失的
Go运行oracle；**没有运行Go测试或声称Go运行差分**。未修改oracle-index证据分类。
[provenance](logs/task_153/provenance.json)记录来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、五份Go来源、fixture与index哈希；
全部来源与该commit逐字节相同。

Go `evictSlowTrendHandler` POST `/config` 要求数值 `recovery-duration`，保存失败回滚，
成功响应200 JSON字符串 `Config updated.`；GET `/list` 使用只复制RecoverySec的clone，
驱逐字段必为null。不能以BaseScheduler的不支持响应代替此专属handler。
未知路径404明文、方法错误405空body且无Allow；宿主负责路由挂载及线上HEAD抑制。
`apiutil.ReadJSONRespondError` 区分400/input与请求读取失败500/internal。

`TestEvictSlowTrendTestSuite` 的BasicFuncs覆盖候选/恢复计时，EvictSlowTrend及V2使用
transientRecoveryGap failpoint验证驱逐和恢复，Prepare覆盖恢复配置接线；没有专属HTTP
方法/错误响应或持久化失败用例。本任务不裸跑依赖failpoint的Go测试，也不把测试名称
当成运行证据。Go源码仅用于设计固定输入/期望。

[HTTP fixture](fixtures/task_153.json)冻结39条来源推导案例，覆盖方法/路径/query、
缺失/null/错误类型、未知字段、重复字段、0、小数、指数、2^53精度损失及固定JSON错误。
成功/路由/列表/业务错误按精确响应字节检查；结构化input错误比较所有JSON字段，
status/header/config及持久化字段不归一化。堆栈仅检查结构和非空。
负数采用相邻适配器的signed-to-unsigned约定；Go超界及负数浮点转换有平台差异，
不声称跨平台运行一致。任意损坏JSON诊断、float64溢出及Gorilla路径清理重定向
不在固定案例一致性声明中。

## 实现与失败—通过验证

生产路径 [schedule_admin_evict_slow_trend.rs](../../../pkg/server/schedule_admin_evict_slow_trend.rs)
提供 `serve(&mut SlowTrendScheduler, &StorageEndpoint, &ClusterPaths, Request<Body>)`，
独立测试在server/lib.rs显式注册；[说明](../../../pkg/server/schedule_admin_evict_slow_trend.rs.md)
记录领域边界和故障语义。

先注册HTTP及故障测试，以错误501成功响应为候选执行
[红测](logs/task_153/red.json)：聚焦命令退出101，0通过/2失败，均由501与200行为
断言检出；不是缺API编译错误。修正为200后
[初次绿测](logs/task_153/green.json)退出0，3通过；补齐活动候选/活动驱逐故障断言后
执行最终完整验证。

[四个真实测试](../../../pkg/server/schedule_admin_evict_slow_trend_test.rs)：

- `task_153_behavior_cases`：39条HTTP输入响应和逐案真实MemoryStore持久化检查。
- `task_153_persistence_and_body_faults`：MetaStore I/O边界Unavailable导致500，内存和
  持久化配置均保留；恢复I/O后更新成功并重建配置。三条不读body路由使用poll即panic
  流，在存储失败时仍正常返回；两个请求读取故障（含完整JSON前缀）均500/internal，
  不应用部分结果，持久化仍为原值。
- `task_153_schedule_after_http_update`：真实core/统计/placement/存储/共享构建器；
  HTTP配置100秒后确认驱逐，暂停目标时无操作，恢复目标后生成3个唯一region的
  Urgent Leader多目标TransferLeader。核对步骤、目标、影响量、confver、两种心跳命令
  以及应用后完成。活动驱逐期间保存失败保留时长、列表和候选时钟；list仍返回null。
  源节点变快但100秒未满时继续驱逐；HTTP改为0后候选时钟和驱逐列表不变，下一轮
  真实调度清理驱逐标记，不发新操作。
- `task_153_captured_candidate_preserved`：尚未收齐心跳的候选经过HTTP更新仍保留
  当前/历史及捕获时间，随后心跳到齐实际确认驱逐，恢复时长仍为更新后的9秒。

时钟固定10^12ns，目标按ID确定、随机索引循环、显式不打乱；没有忽略目标顺序或
错误差异。只有时钟、随机和外部I/O边界被控制，领域状态与操作均使用真实对象。

## 验证与只读审计

[verify.py](logs/task_153/verify.py)每阶段通过原子mkdir顺序动态领取1–10槽位，记录
计划/任务/会话/PID，所有Cargo及子进程继承绝对CARGO_TARGET_DIR。
红测、绿测和最终阶段实际均领取 **槽位1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
默认features；CARGO_BUILD_JOBS=4，RUST_TEST_THREADS=2。
失败/正常/中断均等子进程退出再释放自己的锁，保留缓存。

[只读Go拦截器](logs/task_153/reuse-go/go)拒绝实际Go执行，仅校验并复用已有etcd测试
二进制和已有kvproto ZIP；[复用记录](logs/task_153/go-reuse.jsonl)。没有make、failpoint、
Go构建/测试、生成器或模块写入。该二进制用于Rust测试的外部etcd边界，不是本任务
HTTP契约的Go运行oracle。

[最终命令、退出码和日志](logs/task_153/verified.json)及
[有效测试汇总](logs/task_153/verified-summary.json)：

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务Rust格式变化 |
| `cargo test -p astersql-pd-server --locked --lib task_153 -- --nocapture` | 0 | 4通过、0失败、0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 114通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1914通过、0失败、1既有忽略 |

仅出现既有cpu_usage弃用warning，无新增忽略。
[审计脚本](logs/task_153/audit.py)及[结果](logs/task_153/audit.json)与
[开始快照](logs/task_153/baseline.json)核对全部Go/模块文件集合、内容哈希、Git状态
（含untracked）及两份只读plan，共1,103个文件；内容与状态均不变。
验证日志与provenance哈希、Markdown链接及git diff --check均通过。
最终自有槽位锁已释放，保留缓存。只提交本任务适配、测试、文档和证据，排除其它任务
已有删除；完成后删除153编号任务文件，不修改plan或原主题。

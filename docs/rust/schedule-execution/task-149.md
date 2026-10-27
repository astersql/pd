# 任务149：热点扰动配置传输适配

状态：已完成。前置148提交 `9398c4dfd`、[task-148](task-148.md)与完整验证日志已核实。
复用 canonical ShuffleHotScheduler 与已有 storage/core/statistics/placement API；
未新增crate、feature或依赖。只读plan、原主题、Go与模块文件保持原样。

## 来源与适用边界

用户明确授权允许只读Go源码/测试推导加Rust真实行为和故障验证，替代缺失的Go运行
oracle/运行差分；没有执行Go或声称取得Go运行结果。
[provenance](logs/task_149/provenance.json)冻结source_commit、来源/fixture/oracle-index SHA256。
四份Go来源与该commit逐字节一致。oracle-index的149项原本只有source_assertion，
未改写其runtime_fixture字段。固定[HTTP场景](fixtures/task_149.json)标记
`runtime_go_oracle=false`；原始method/path/input/status/body/config均可审查。

只读 shuffle_hot_region.go 的专属handler决定POST `/config`和GET `/list`、
limit字段数字解码/截断、无类型时400、成功200/null、保存失败500且回滚。
BaseScheduler的默认200明文不适用于此scheduler。
apiutil.go决定完整读body后解码、400/input与500/internal错误header/结构。
scheduler_test.go的 `TestShuffleHotRegionScheduleBalance` 覆盖placement开/关和
新peer/leader目标，但不覆盖HTTP更新或保存失败；没有把该名称当成运行证据。
调度输入/输出复用[task100 fixture](fixtures/task_100.json)的18条源码推导案例，
包含来源相关的正常、边界和故障，期望并非从本次Rust实现反推。

对象JSON按字段语义比较，列表另验证精确缩进字节；status/header/字符串body不归一化。
只对语言相关错误stack比较存在和结构。保存故障文本来自实际Rust MetaStore错误，
Go源码证明保存错误被透传，不声称该文本来自Go运行。
时间固定100，peer ID分配100；独立随机输入与排序后store选择沿用task100公开边界。
负数到uint64沿用相邻适配器的signed-wrap约定；Go超界转换具有平台相关性，
负数案例只验证该明确Rust约定，不声称跨平台Go结果。非负可表示数字经过float64
舍入再截断，覆盖2^53精度边界。超出float64范围、任意坏JSON全部解析诊断、
Gorilla清理/重定向不在固定案例一致性声明中，没有忽略错误或操作差异。

## 生产入口与测试驱动

[生产适配](../../../pkg/server/schedule_admin_shuffle_hot_region.rs)公开：
`serve(&mut ShuffleHotScheduler, &StorageEndpoint, &ClusterPaths, Request<Body>) -> Response`。
[边界说明](../../../pkg/server/schedule_admin_shuffle_hot_region.rs.md)解释专属handler、
数值转换、事务可观察状态与宿主路由职责。lib.rs显式注册生产模块及独立测试。
领域已有update写成功后发布，借用覆盖I/O；失败保留旧值，与Go锁内更新失败回滚等价。
未装配主题7的完整server/api路由。

[红测](logs/task_149/red.json)先运行同名真实行为测试，错误候选返回501，
在limit=0成功请求的200断言处失败：exit101，0通过/1失败。
修正为200后相同测试通过。focused日志另保留新增调度测试中局部变量遮蔽导致的
编译错误及修正；该错误未作为所需行为红测。

[独立测试](../../../pkg/server/schedule_admin_shuffle_hot_region_test.rs)：

- `task_149_behavior_cases`：38条HTTP案例，含路由/方法、非法类型、null/缺字段、
  大小写/未知/重复字段、小数/科学记数/0、数值精度/可表示上边界、语法与顶层错误。
  校验status/body/content-type/error header、无Allow、内存与真实MemoryStore配置。
- `task_149_persistence_and_body_faults`：MemoryStore仅I/O边界注入Unavailable，
  保存失败500且内存/持久化旧值不变，reload后重试成功，重新构建恢复limit=8；
  3条列表/拒绝路径在存储故障和一旦poll即panic的body下验证不访问外部依赖；
  2条空/完整部分输入流失败校验500/internal、配置与持久化均不应用部分输入。
- `task_149_schedule_after_http_update`：18条task100场景×placement开/关×conf-change
  v1/v2，共72条真实调度案例。每条先检查limit=1在count=1被阻断，HTTP更新至2、
  reload后准入并实际调度；同时验证hot/region/leader严格限额。最后HTTP更新至0
  验证无操作准入。使用真实BasicCluster/RuleManager/StoreLoadDetail/操作builder，
  验证步骤、源/目标、region、peer分配、kind、Low、confver、影响量、heartbeat命令。
  覆盖无热点、missing/down/pending、无合法目标、驱逐/offline、placement退化和allocator故障。

## 运行与审计

[verify.py](logs/task_149/verify.py)通过原子mkdir依次动态领取仓库共享槽位1–10，
记录计划/任务/会话/PID。所有阶段实际槽位 **1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
默认features，CARGO_BUILD_JOBS=4，RUST_TEST_THREADS=2。
正常/失败/中断均先结束并等待子进程，再核对owner释放自有锁；保留缓存。
[Go拦截器](logs/task_149/reuse-go/go)拒绝实际Go命令，仅校验后复用已有etcd二进制
SHA256 `57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`
与固定kvproto ZIP。[复用记录](logs/task_149/go-reuse.jsonl)记录路径和摘要；
没有Go生成、构建、测试或failpoint写入。

[最终确切命令、退出码与原始日志](logs/task_149/final.json)、
[有效测试汇总](logs/task_149/final-summary.json)：

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务差异 |
| `cargo test -p astersql-pd-server --locked --lib task_149 -- --nocapture` | 0 | 3通过、0失败、0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 98通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace元数据通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1898通过、0失败、1既有忽略 |

仅既有cpu_usage弃用warning，本任务未新增忽略或跳过必需检查。
[审计](logs/task_149/audit.json)对照[开始快照](logs/task_149/baseline.json)：
1101份Go/模块文件加2份只读plan集合、哈希及Go/模块Git状态不变，包含untracked扫描；
来源与日志哈希、本地Markdown链接有效，git diff --check退出0。
Cargo清单与锁无差异，唯一共享入口差异是本任务模块/测试注册。
只提交本任务代码、测试、说明、fixture和证据；已有其他编号任务删除全部排除。
验收后删除149任务文件，不改只读plan或来源主题。

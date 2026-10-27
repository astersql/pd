# 任务137：领导者均衡配置传输适配验收

状态：已完成。前置136提交 `75ffb0330` 与[恢复验收证据](task-136.md)已核实。

## 来源与证据边界

按用户后续授权，采用**只读 Go 源码/测试推导 + Rust 真实行为与故障验证**，
替代缺失的 Go 运行 oracle；没有新增或宣称 Go 运行差分。
[provenance](logs/task_137/provenance.json)冻结来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、7份 Go 源码/测试哈希、
[fixture](fixtures/task_137.json)、136证据和准入 oracle 索引哈希。
全部 Go 来源与该基准相同。两级 plan.md 保持只读，来源主题不改写。

Go `newBalanceLeaderHandler` 明确注册 POST `/config` 与 GET `/list`；
`update` 在 batch 非法时恢复旧参数，未知字段/相同配置有独立消息，保存失败仍返回
200 并保留内存配置。`clone` 返回参数而非包含 disabled 的基类。
`TestBalanceLeaderBatchLimit` 只覆盖 max/max+1 HTTP batch；ConfigClone 验证复制隔离；
TestMain 负责 leak/etcd；leader/range suite 为真实调度案例提供源码依据。
不将这些名字冒充实际 Go 运行覆盖。该 scheduler 存在专属接口，故 BaseScheduler
的 `not implements` 不适用于本任务端点。

延续任务071的已验证领域 API 边界：只接受可表示的、batch合法的领域配置，
不宣称兼容 Go 错误解码后留下非法 batch 的任意混合字段部分更新副作用。
本任务实际验收范围是下列明确冻结的请求/状态/JSON和正常、边界、错误、I/O故障案例。

## 生产路径与案例

新增 [serve](../../../pkg/server/schedule_admin_balance_leader.rs)，以独占借用的
canonical `BalanceLeaderScheduler`、宿主 `StorageEndpoint`/`ClusterPaths` 和 HTTP
Request 为输入，输出 Response；局部路径分派不新增主题7的 server/api 全局装配。
配置更新调用既有 `update_leader_config`，不复制领域状态或持久化逻辑。
[实现说明](../../../pkg/server/schedule_admin_balance_leader.rs.md)记录边界。

验收同时补齐显式空 ranges 的必要依赖：`set_ranges` 和 serde 保留空列表，
使 HTTP list 和存储重载保留 `[]`。Go `regionTree.RandomRegions` 的 `len(ranges)<=1`
快速路径在空列表时保留全树默认区间；Rust继续采用相同的全键空间采样语义。
本fixture的注册key-range manager为空；不宣称修复Go空列表与非空manager组合的索引问题。
constructor默认范围保持原有语义，调度继续走真实过滤、规则与操作工厂。

[显式注册的测试](../../../pkg/server/schedule_admin_balance_leader_test.rs)有4个函数，
合计46个有效案例，没有新增忽略测试：

- `task_137_behavior_cases`：33个顺序 HTTP 案例；list/更新、query、405/404、HEAD、
  OPTIONS、未知字段、空/null请求、大小写、相同配置、batch 0/-1/101/100/1、
  数字/字符串类型错误、空/未完成JSON、范围hex/类型错误、空范围与更新后list。
  逐项比较状态、JSON值/消息、JSON content-type、路由原始body、领域batch。
- `task_137_updated_scheduling_cases`：7个 HTTP → 真实调度 → 存储重载调度案例；
  batch 1/4/100分别生成1/4/8个操作，范围外0、单范围1、多范围2、空范围1（batch=1，全键空间采样）。
  每个真实操作均为 TransferLeader，来源1，目标为有同等资格的2或3，region唯一，
  位于指定范围，且不污染输入 pending influence。更新前基准始终生成4个操作。
- `task_137_persistence_failure`：4个阶段；MemoryStore 外部 I/O 边界故障时更新
  仍200且active=1；相同请求与list仍成功；后端恢复后旧持久化值仍为4，reload恢复4；
  再次正常更新并reload得到100。没有以mock调用次数代替状态验证。
- `task_137_body_read_failure`：2个阶段；流的完整JSON字节后发生读错误仍更新为2；
  不完整JSON后发生读错误返回500并保留2，复现Go忽略ReadAll错误后的解码入口。

真实组件包括 BasicCluster、三节点/八region、RuleManager、RegionLabeler、
StatisticsSnapshot、OpInfluence、MemoryStore/StorageEndpoint。仅显式时间、随机排列
与外部 I/O 故障受控。JSON只归一化空白和对象键顺序；原始404/405 body严格比较。
目标2/3是Go未指定并列顺序的合法集合，不作为忽略目标差异的归一化。

## 失败与通过证据

[compile](logs/task_137/compile.json)记录新增依赖边初次 `--locked` 失败；
[red](logs/task_137/red.json)包含离线更新、格式化与首次真实行为失败：
同一 `task_137_behavior_cases` 运行1个测试、退出101，GET `/list` 实际404而期望200。
锁更新顺带产生的 pulldown-cmark 漂移已还原，最终没有远程依赖版本变化。

[green](logs/task_137/green.json)和[focused](logs/task_137/focused.json)保留初次
所有权/重复导入错误；[green2](logs/task_137/green2.json)保留fixture默认值接线错误。
这些编译/测试接线错误均不计为行为失败证据。
[green3](logs/task_137/green3.json)记录测试设计错误：最初误把空列表当成零候选，
而实际产生1个操作。复核Go `regionTree.RandomRegions` 快速路径后，修正源码推导期望
为1，并撤回曾临时增加的“空列表返回零操作”改动；最终没有修改调度算法。
该日志不计为有效行为失败证据，初次404断言失败才是本任务红验证。
[interrupted](logs/task_137/interrupted.json)保留修正前的部分基线；发现该错误后
终止并join所有子进程、释放自己的槽位，再对修正后的代码完整重跑最终基线。
[corrected-fixture](logs/task_137/corrected-fixture.json)记录修正计数后同步修正
范围断言的中间失败；空列表案例应允许全键空间，无需检查空列表中包含目标范围。

[final](logs/task_137/final.json)保存全部确切命令、退出码、有效测试摘要、
压缩完整stdout/stderr及日志哈希；[audit](logs/task_137/audit.json)核对结果。

| 命令 | 退出码与有效结果 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo test -p astersql-pd-server --locked --lib task_137 -- --nocapture` | 0；4通过，0失败，0既有忽略；46有效案例 |
| `cargo test -p astersql-pd-server --locked` | 0；55通过，0失败，0既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0 |
| `cargo test --workspace --locked` | 0；1855通过，0失败，1既有忽略 |

所有 Cargo 调用由[槽位脚本](logs/task_137/verify.py)以原子mkdir动态领取共享槽位；
本次阶段实际为槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划、任务、会话与PID；正常、错误或信号中断均先等待/终止并join子进程组，
再删除自己的owner并rmdir锁。缓存保留。

workspace测试复用任务030已保存的etcd二进制，SHA256为
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`；
[Go拦截器](logs/task_137/reuse-go/go)仅复制该固定二进制并拒绝实际Go命令，
[复制记录](logs/task_137/go-reuse.jsonl)保存调用。未运行Go测试、生成器、模块整理、
格式化或failpoint启停。所有新增实现和测试均为Rust。

[审计脚本](logs/task_137/audit.py)核对所有1101份Go/模块文件集合、内容和Git状态，
两级只读计划、来源/fixture/日志哈希、链接、远程依赖不变、槽位释放与diff。
既有其它编号任务删除不纳入本任务提交。

# 任务 009：位置与规则保护过滤

状态：已完成。批次 9，已核验批次 8 提交 `0992b0b20`、其 138 组/276 次
运行 oracle、14 项包测试和 workspace 证据；没有以编号任务
文件删除代替依赖准入。依赖 API：Filter、core distinct_score、RegionInfo、
placement RegionFit::replace 与 RuleManager::fit_region 均复用 canonical 实现。
两级及其它子计划 plan.md、来源主题均只读。

## 实现与边界

[placement_guard.rs](../../../pkg/schedule/filter/placement_guard.rs)公开：
DistinctScoreFilter/LocationPolicy、RuleFitFilter、RuleLeaderFitFilter、
RuleWitnessFitFilter、PlacementGuardContext 及 create_region_for_rule_fit。
[领域说明](../../../pkg/schedule/filter/placement_guard.rs.md)记录接口、
错误和所有权。位置保护接受相等，改善必须严格增加；peer 保护复用真实规则
替换，leader 拒绝已有 witness，并按 allow_move 控制 region 外目标；witness
只接受已有非 leader peer。Source 始终 OK；完整保留目标状态码和空详细原因，
identity 为 2/4/5/4。未知 source、缺失 peer、规则冲突和隔离退化均实际拒绝。

构造上下文复用宿主 config、store/region 快照、真实 RuleManager 和显式时钟，
可以复用调用方提供的 fit；规则关闭时 peer 回退到位置保护，leader/witness
返回 None。没有新 feature、全局状态、私有 runtime、状态副本或协议领域类型。
既有 placement -> storage-endpoint -> proto 传递链不变。新增三个 dev dependency
仅用来装配真实 MemoryStore/StorageEndpoint/ClusterPaths 测试 RuleManager；
Cargo.lock 只增加本地依赖边并排序，离线生成带来的 pulldown-cmark 更新已在
验证前恢复原锁定版本，没有远程依赖漂移。

临时 region 完整复制 leader、重建 peer id/store/role 并清除 peer witness，按
顺序应用 RegionOption；ID/epoch/health/stats 使用默认值。测试同时检查原 region
保持不变、learner 保留、leader witness 独立保留、替换 store/key 和空输入。

## Go oracle

来源 commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`。
[fixture](fixtures/task_009.json)、[provenance](logs/task_009/provenance.json)、
[来源哈希](logs/task_009/sources.json)记录原始领域输入、二进制 SHA256 和归一化。
复用 `/tmp/pd-sched005/filter.test`、`schedulers.test`、`dlv`，未创建、改写或
重新构建任何 Go 文件。TestMain、WaitForEtcdConnections、选定 setup 与断言
路径经只读审查，使用路径不需要 failpoint 注入。

[freeze.py](logs/task_009/freeze.py)和
[guard-boundaries.py](logs/task_009/guard-boundaries.py)实际冻结 129 组、258 次
source/target 状态返回：

| 路径 | 案例 | 覆盖 |
| --- | ---: | --- |
| location0/1/2 | 36 | 三种真实 region 分布，6 个目标，各 safeguard/improve；源自身、已有目标、改善、相等、退化 |
| default_rules | 42 | 7 个 store，存在/缺失 source，peer 与两种 leader allow_move |
| explicit_rules | 36 | leader/voter 跨 dc 规则，6 个目标，存在/缺失 source，三种保护方式 |
| witness | 9 | 真实 scheduler fit，现有 follower/witness、leader、缺失 source 与缺失 target |
| leader_witness | 2 | 把已有 peer 的 witness 输入置真，独立于隔离失败，验证两种 allow_move 都拒绝 |
| nil_leader | 4 | nil leader 的安全 GetId=0 语义，四种现有 peer |

脚本只改变领域输入并调用真实生产方法。witness 在现有 scheduler 测试 setup
之后显式启用 placement rules；不冒称改变输入后的整项 scheduler 测试通过。
leader_witness 复用原 fit，改变 region peer 输入以独立检验 witness gate。
原始返回、确切 Docker/Delve 命令和退出码分别保存在各 route 的 commands/json。
归一化仅排除调试指针、栈帧与日志时间；完整保留 StatusCode/DetailedReason。
witness scheduler 动态分配的 peer ID 在 fixture 中按 store ID 做双射重命名，
leader 仍指向 store 3，原始 ID 保留在 witness.json；不改变相等关系或规则角色。该 witness 规则恰好选择全部四个 peer，因此
重命名不会改变组合选择；provenance 保存 25→3、26→1、27→2、28→4 的实际映射。
没有忽略错误、目标或操作差异。

[extra-oracle.py](logs/task_009/extra-oracle.py)另外保存真实 temporary helper
返回，[temporary.json](logs/task_009/temporary.json)记录原 witness peer 为 true、
副本为 false、原 region 调用后仍为 true，leader/peer 输入与完整返回均保留。
额外 ownership/option 组合按源码契约设计 Rust 性质测试，未冒称每个组合都有
Go 运行差分。无存储 I/O 的过滤器不构造无关持久化失败。
[safe-tests.json](logs/task_009/safe-tests.json)另行完整运行 6 项未修改 Go 测试，
退出 0：DistinctScore、LabelConstraints、RuleFit、RuleFitWithPlacementRule、
SendState、PlacementGuard。其中 LabelConstraints/SendState 属于相邻回归，
不把其测试名当成本任务的直接行为证明。

## TDD 与错误行为验证

[task_009_test.rs](../../../pkg/schedule/filter/task_009_test.rs)先注册并取得
[缺失 API 失败](logs/task_009/red-compile.out)，退出 101；该零行为测试编译错误
不当作有效失败断言证据。最小实现后首轮 123 组通过；随后增加 6 组 gate 边界，
以及独立 temporary 和真实 MemoryStore-backed RuleManager 工厂/复用测试。
第二轮编译暴露 StringSlice 和 initialize store slice 类型输入问题，已修正。

[mutation.py](logs/task_009/mutation.py)对同一注册测试逐项引入并还原 8 个错误：
safeguard 拒绝相等、improve 接受相等、绕过规则替换、leader 接受 witness、
leader 允许缺失 peer、witness 接受 leader、temporary 保留 peer witness、
忽略提供的 fit。八次均触发真实断言失败并退出 101；
[mutation.json](logs/task_009/mutation.json)保留逐字 stdout/stderr 与有效失败数。
[mutation-stage.out](logs/task_009/mutation-stage.out)记录阶段最终退出 0，源码
在 finally 中恢复，恢复后重新格式化并通过所有最终验证。

[fault.py](logs/task_009/fault.py)取得空 store 集合构造位置保护时 Go 的
`makeslice: cap out of range` panic；[fault.json](logs/task_009/fault.json)保存
实际调用和返回。补充的同一注册测试先取得[断言失败](logs/task_009/red-empty.out)
（3 通过、1 失败，退出 101），再在构造器添加非空前置检查。最终 4 项测试
同时验证 safeguard/improve 都保留此失败语义，不比较语言特有 panic 文字。

## 最终验证

[validation.py](logs/task_009/validation.py)、
[validation.json](logs/task_009/validation.json)保存实际命令、逐字 stdout/stderr、
退出码和测试计数；validation-N.out 是仅修剪尾部空白的显示日志。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-filter --locked --lib task_009 -- --nocapture` | 0 | 4 通过，0 失败/忽略；129 组/258 返回 |
| `cargo test -p astersql-pd-schedule-filter --locked` | 0 | 18 通过，0 失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace 编译 |
| `cargo test --workspace --locked` | 0 | 1344 通过，0 失败，1 既有忽略（tso_processes） |

[slot-run.py](logs/task_009/slot-run.py)每阶段按原子 mkdir 依次尝试共享槽位
1–10，记录 task/session/PID，在正常/错误/信号路径等待子进程结束后释放自有锁。
全部 Cargo 阶段（含失败、锁文件、格式化、变异和最终回归）实际领取槽位 1，
绝对 CARGO_TARGET_DIR 为 `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[validation-stage.out](logs/task_009/validation-stage.out)记录最终领取和命令退出；
证据审计确认没有本任务残留锁，构建目录仍存在，
缓存保留，没有删除他人锁。

[verify.py](logs/task_009/verify.py)重新解析所有原始 Go 返回并核对 fixture，
核验 Go/模块文件起止哈希（含 untracked/ignored）、17 份只读 plan.md、二进制
SHA256、远程依赖版本、生产依赖方向、文档链接、变异及有效测试数量。
[Go 审计](logs/task_009/go-verification.json)和
[证据审计](logs/task_009/evidence-verification.json)保存结果。
`git diff --check` 通过。本任务只提交相关 Rust/接线/测试/证据，完成后删除编号
任务文件，不改任何 plan.md 或来源主题。

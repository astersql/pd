# 任务156：全包覆盖审计与主题回归

状态：已完成。初次存储适配层依赖阻塞已按用户后续授权修复；严格crate normal传递
依赖、全包覆盖及所有最终验证通过。两份plan.md和原始主题6只读；58/67编号文件由主会话处理。

依赖155由提交 `44c6366cb`、[任务155](task-155.md)与全部8条最终命令的日志哈希核实，
详见[依赖核验](logs/task_156/dependency-155.json)。初次验收提交为 `d25981c4e`。
按用户明确授权，以只读Go源码/测试推导加Rust真实行为/故障验证替代缺失Go运行oracle，
没有新增Go运行差分；[oracle-index](oracle-index.json)原分类保持不变。

## 最小依赖修复与兼容边界

原始[失败依赖审计](logs/task_156/pre-split/dependency-audit.json)发现75条到proto/prost/tonic
的normal路径，例如 `schedulers → storage-endpoint → proto`、`core → id → storage-kv → tonic`。
不是unused依赖，直接删除会破坏实际适配器功能；初次结果没有冒充完成。
后续用户明确授权必要的Rust契约/适配器拆分，保留严格门槛和现有后端行为。

新增唯一的中立存储crate：
[storage-domain](../../../pkg/storage/domain/lib.rs)，领域说明见
[lib.rs.md](../../../pkg/storage/domain/lib.rs.md)。它显式注册原KV中立实现文件及原endpoint的
字节/JSON、配置、调度器、规则、affinity、batch实现，不复制业务算法。旧KV facade
重导出同一套MetaStore、Base、MemoryStore和事务类型，同时保留Etcd实现及全部测试。
旧endpoint保留prost/wire实现、region ID游标；字节/JSON方法委托同一MetaStore的中立句柄。

[中立KV](../../../pkg/storage/domain/kv.rs)与[中立endpoint](../../../pkg/storage/domain/endpoint.rs)
不依赖protobuf、prost、tonic、具体Etcd、server或runtime。EndpointError保留错误分类和显示
文本；适配器的底层codec错误通过标准Error trait对象承载，真正编解码仍在原endpoint。
隐藏的public校验/range/取消辅助函数供Etcd适配器复用，避免复制算法。

调度、placement、affinity、labeler、statistics、cluster-utils改用中立crate；传递路径上的
ID/member/election也切到同一中立契约，Etcd测试依赖留在dev-dependencies。
`RuleManager::new`、`FactoryContext::new`、配置/range构造器通过AsRef接收新旧句柄；
`RegionLabeler::load`通过Into接收。克隆只共享后端Arc，不复制或重建后端状态。
旧Etcd、protobuf功能、存储契约和回归测试均保留。没有新增feature或远程依赖。

[锁文件审计](logs/task_156/lock-audit.json)确认全部远程包source/version/checksum及依赖块
与d25981c4e相同。首次离线生成暂选pulldown-cmark 0.13.5，已恢复原0.13.4及全部原远程块；
最终只改变workspace包及依赖接线，后续验证使用--locked。
[修复provenance](logs/task_156/repaired-provenance.json)固定实际修改、新中立文件、Go wire
fixture和来源哈希；[初次provenance](logs/task_156/provenance.json)保留修复前快照。

## 真实验收与故障测试

[schedule_acceptance_test.rs](../../../tests/rust-compat/schedule_acceptance_test.rs)
在compat Cargo.toml显式注册为`schedule_acceptance`。使用既有真实BasicCluster、18类
SchedulerRegistry策略、SchedulerController和MetaStore；期望复用先前冻结的
[task106 fixture](fixtures/task_106.json)及[task156场景](fixtures/task_156.json)。

4个独立测试：

- `task_156_behavior_cases`：18类注册集合、有效args、名称、默认字段与range job字段；
  真实保存后重建registry/endpoint并逐字节核对恢复，验证损坏配置及修复。
- `task_156_pause_resume_all_factories`：18类各用独立真实cluster，检查add、按名暂停、
  到期前一秒/精确到期、all暂停、显式恢复、remove，以及移除后再次暂停报错。
- `task_156_io_faults_and_invalid_configuration`：18类read/transact边界失败（36次I/O拒绝），
  核对错误、旧提交字节与恢复重试；8条非法args和1条未知name。正常I/O委托真实MemoryStore。
- `task_156_storage_adapter_shares_neutral_backend`：检查旧KV与中立类型身份相同，旧adapter、
  中立句柄及FactoryContext读到同一配置；使用既有冻结Go request_header wire字节验证
  原protobuf适配器的实际保存/加载；坏wire保持ProtoUnmarshal分类、文本及原prost错误类型。

前三项共54条按工厂划分的故事与9条独立错误案例；它们不是54个libtest函数或18个线上
集群。真实Schedule步骤/kind/priority/目标/影响量、错误和限流由各策略既有测试在本次
workspace重跑。新增第四项单独验证拆分边界，不能只靠编译或对象创建证明后端共享。

transfer-witness的Go工厂`init.go:552–557`忽略decoder，因此损坏配置恢复为null；另外17类
拒绝。该例外明确固定并验证，没有修改正确生产行为。其队列为瞬态，gate恒true，实际
store预算和队列边界由task103覆盖；不虚构队列持久化故事。18类均实测宿主暂停/恢复。

## 来源覆盖、架构与性能

[逐来源映射](logs/task_156/source-coverage.json.gz)核实14包、85个生产文件、1213公开符号、
900内部符号、44指标、24持久化候选、17异步及25failpoint站点的owner与本次通过测试；
2个keypath定义单独关联配置存储测试。每项都有owner证据和fixture哈希，task002–155
均有本次行为测试通过。来源Go文件集合和SHA256无漂移。持久化候选包含history.Remove
等启发式命中，不能称24个存储事务。

[18类矩阵](logs/task_156/scheduler-matrix.json)关联实际Schedule结果、策略错误/限流/配置、
共同工厂保存/恢复和本次暂停测试；逐包覆盖heartbeat classic/v2、闭环、影响量、I/O取消、
重启与性能。这是来源归属到行为套件的可追溯性，不是每条语句/分支动态覆盖率，也不
声称每个符号各有独立Go运行差分。

[最终依赖审计](logs/task_156/dependency-audit.json)采用与初次失败相同的禁止集合，增加
中立存储入口，共审查23个领域入口及其全部normal workspace传递边；不能用仅直接边、
feature或过滤器跳过存储路径。真实Etcd/codec的dev测试和适配crate仍保留在完整cargo tree。

[性能重放](logs/task_156/performance-regression.json)重跑155的36条样本、4条错误/边界与
666步leader收敛（最终`[334,333,333]`）。分数、改善量、操作数和rank比较保持严格；耗时/
分配量只记录，没有Go耗时对比或新增阈值。跨进程等分source可变，步骤差异逐项保留，
不声称字节相同：Go balance_region按分数排序无ID tie-break，原155测试允许移出1..3，
并按实际source检查kind/步骤。同一快照冷/热步骤严格一致；只沿用原先声明的空目标tie
归一化，不把不同source的操作归一成相同。仍是debug合成快照与MemoryStore契约测试，
不是线上吞吐量或生产durability证明。[环境](logs/task_156/environment.json)注明机器未独占。

## 红灯与验证记录

初次[缺target](logs/task_156/red-registration.json)退出101；
[错误decoder期望](logs/task_156/initial.json)2通过/1失败，核对Go源码后修正。
[初次错误变异](logs/task_156/red.json)污染恢复字节、取消暂停、禁用I/O故障，0通过/3失败，
exit101；[快照](logs/task_156/mutated-test.rs.gz)保存当时实际源码。

拆分新增[错误后端变异](logs/task_156/split-red.json)：给旧适配器的中立句柄新建独立后端，
同一4测试出现1通过/3失败，exit101；配置恢复、I/O错误和冻结wire断言均检出问题。
[变异脚本](logs/task_156/split-red.py)与[源码](logs/task_156/split-mutated-adapter.rs.gz)可审查；
finally先等待Cargo子进程退出再恢复源码。没有Go文件改写。

修复期间编译/接线失败均保留在split-initial、split-check2/3/4、split-closure记录；
它们不是最终通过证据。[原存储包测试](logs/task_156/split-tests.json)包含真实MemoryStore、
Etcd、事务取消/恢复、wire codec测试；[全仓目标编译](logs/task_156/split-compile-all.json)
只用于提前发现跨crate接线问题，不替代行为回归。

[最终命令记录](logs/task_156/repaired.json)、[完整数量汇总](logs/task_156/verified-summary.json)
保存确切命令、stdout/stderr、退出码和日志SHA256。性能target单线程，其余测试2线程，
CARGO_BUILD_JOBS=4；默认features。metadata/fmt/check/test/tree及58/67、性能均重跑。

[runner](logs/task_156/verify.py)按1–10原子mkdir动态领取槽位，记录task/session/PID；
全部Cargo和子进程继承绝对CARGO_TARGET_DIR，正常/失败/中断均等子进程退出后只清理
自己的锁。实际槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，缓存保留。
[Go拦截器](logs/task_156/reuse-go/go)拒绝实际Go构建/测试，仅复用已核验的现有etcd二进制
和只读kvproto ZIP；[调用记录](logs/task_156/go-reuse.jsonl)可复核。

[最终审计](logs/task_156/audit.json)及[执行记录](logs/task_156/audit-run.json)由
[audit.py](logs/task_156/audit.py)核对覆盖、依赖、性能质量、日志/来源哈希、Markdown链接、
Go/模块集合和Git状态。1983个Go/模块文件（含ignored/untracked）及两份plan保持原样。
初次失败结果完整保存在[pre-split](logs/task_156/pre-split/audit.json)，不覆盖为假通过。

## 最终结果

| 命令 | 退出码 | 实际结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-compat-tests --locked --test schedule_acceptance -- --nocapture` | 0 | 4通过、0失败、0忽略 |
| `cargo test -p astersql-pd-compat-tests --locked` | 0 | 33通过、0失败、1忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1925通过、0失败、1忽略 |
| `cargo tree --workspace --locked` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_058 -- --nocapture` | 0 | 8通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_067 -- --nocapture` | 0 | 3通过、0失败、0忽略 |
| `cargo test -p astersql-pd-compat-tests --locked --test schedule_performance -- --nocapture` | 0 | 3通过、0失败、0忽略 |
| `cargo tree --locked --edges normal -p …`（23入口，完整命令见normal-tree.json） | 0 | 无禁止协议/传输依赖 |
| `python3 docs/rust/schedule-execution/logs/task_156/audit.py` | 0 | 覆盖、依赖、只读、日志与质量审计通过 |

[normal-tree原始记录](logs/task_156/normal-tree.json)保留完整包参数与Cargo实际解析树；
与metadata的normal边遍历互相核验，不以只列crate名字代替依赖证据。
原存储包另有143项实际测试通过；完整workspace仍执行它们，无删除功能或忽略测试。
只有既有`tso_processes`保持忽略；新验收测试没有忽略项。

任务156的验收证据已齐备，主题具备本任务要求的完成证据。主会话仍负责58/67编号
文件的独立归档以及原始主题6的来源完成流程；本任务不代改、删除这些文件。
只提交必要Rust修复、测试、接线和证据，排除所有其他编号任务及用户变更。
自己的槽位锁已释放，编译缓存保留。156编号任务按契约删除，不单独提交状态/删除。

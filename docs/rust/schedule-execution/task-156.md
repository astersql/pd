# 任务156：全包覆盖审计与主题回归

状态：已阻塞。全部Cargo验证通过；主题必需的normal传递依赖审计失败。编号任务保留。

依赖155由提交 `44c6366cb`、[任务155](task-155.md)与全部8条最终命令的日志哈希核实，
详见[依赖核验](logs/task_156/dependency-155.json)。整体与包级plan只读；没有子代理。
任务58、67由主会话负责最终状态清理，本任务只提供它们的新回归证据，不改其文件。

本任务遵照后续明确授权，采用**只读Go源码/测试推导 + Rust真实行为及故障验证**
替代缺失Go运行oracle。没有执行新的Go oracle或宣称新Go/Rust运行差分；
[provenance](logs/task_156/provenance.json)固定来源commit、源码与冻结fixture哈希。
既有Go运行证据仍按[oracle-index](oracle-index.json)原分类保存，没有重分类或伪造。

## 新验收测试与生产路径

[schedule_acceptance_test.rs](../../../tests/rust-compat/schedule_acceptance_test.rs)
通过compat Cargo.toml显式注册为 `schedule_acceptance`，默认features；无需lib.rs重复注册。
复用现有canonical `SchedulerRegistry::{register_builtins,create_args,restore}`、
`RegisteredScheduler::{encode_config,save_config}`、
`SchedulerController::{add,pause_or_resume,is_paused,delay,remove}`、真实BasicCluster、
MemoryStore/StorageEndpoint。没有新增生产算法、crate、依赖、feature或Go文件。

[任务156 fixture](fixtures/task_156.json)冻结18类数量、暂停时钟100秒/TTL10秒、110秒
到期、损坏配置和decoder例外。有效args、名称、字段、job期望复用先于本任务冻结的
[任务106 fixture](fixtures/task_106.json)，其哈希已固定；不由被测Rust实现生成期望。

3项独立测试：

- `task_156_behavior_cases`：18条配置故事。核对注册类型集合、名称、关键默认值与range
  job字段；经真实后端保存后重建registry和endpoint，逐字节比较恢复结果；损坏配置
  后验证17类拒绝、transfer-witness例外，再修复并重载全部18类。不归一化配置差异。
- `task_156_pause_resume_all_factories`：18条生命周期故事，各用独立真实cluster，
  add→按名暂停→到期前1秒仍暂停→精确到期恢复→all暂停→显式恢复→remove；
  验证delay、移除后的空集合与再次暂停错误。操作器数量/预算限流另由下述策略测试验证。
- `task_156_io_faults_and_invalid_configuration`：18条故障故事，每类各注入一次read与
  transact边界Unavailable，保存/恢复返回错误；恢复I/O后核对旧提交字节及成功重试。
  I/O adapter正常路径全部委托既有MemoryStore。另有8条非法args、1条未知name错误。

共54条按工厂划分的故事与9条独立非法输入/名称案例；其中故障故事包含36次I/O拒绝。
这不是54个独立libtest函数，也不是18个完整线上集群模拟。实际Schedule操作步骤、kind、
priority、目标、影响量、错误和限流由每类真实策略的既有行为测试在本次workspace重新执行。

首轮新增测试错误地要求全部18类拒绝损坏配置；实际transfer-witness工厂在Go
`init.go:552–557`忽略decoder，Rust既有实现与之相同。对照来源修正fixture，明确验证
其null配置，不改正确生产行为。这个策略没有可持久化的调度队列，队列为瞬态；其
`is_schedule_allowed`恒true，实际store预算与队列边界由task103测试覆盖，不虚构通用限流。
所有18类的宿主级暂停/恢复均适用并已实测。

## 覆盖与架构审计

[逐来源覆盖映射](logs/task_156/source-coverage.json.gz)核验准入基准后的实际文件集合、
逐文件SHA256和每项任务归属：14个来源包、85个生产文件、1213个公开符号、900个内部
符号、44个指标构造、24个持久化候选、17个异步站点、25个failpoint站点。
每项都关联owner任务证据、fixture哈希与**本次**通过的任务测试；2个keypath定义单独
关联配置保存/恢复测试。持久化候选包含源码启发式匹配，如history.Remove，不能把24
解释为24个实际存储事务。来源文件与准入SHA256一致，无增删或源码漂移。

[18类矩阵](logs/task_156/scheduler-matrix.json)分别关联真实Schedule结果测试、
策略限流/错误/配置任务、共同工厂持久化与本次暂停/恢复测试。balance、evict、grant、
hot、label、merge、scatter、shuffle、witness、range所有工厂均有新通过结果，
不是仅创建对象、列测试名或复用旧“通过”状态。task002–155每个任务都有新通过测试；
逐包证据包括heartbeat classic/v2、闭环、影响量、I/O取消、重启及性能，详见逐项映射。

此审计是**来源归属到行为测试套件的可追溯性**，不是每条语句/分支的动态覆盖率测量，
也不声称每个公开符号各有独立运行差分。metric label/help/type、key及后台任务由其owner
真实行为套件验收；本任务不复制这些生产逻辑或给源码推导套上Go运行标签。

[依赖审计](logs/task_156/dependency-audit.json)结合新metadata和完整cargo tree，沿22个
调度领域及cluster-utils/progress/operatorutil的normal workspace传递边审查，发现75条
到proto/prost/tonic的路径（含共享终点的多条路径，不是75个独立缺陷）。例如：

- `schedule-schedulers → storage-endpoint → astersql-pd-proto/prost`
- `schedule-schedulers → storage-endpoint → storage-kv → tonic`

直接领域Cargo依赖没有新增transport，但传递图仍不满足本任务“无领域到proto/transport
依赖倒置”的最终门槛。既有storage-endpoint的 `endpoint_codec.rs`、`keyspace_meta.rs`
等实际使用prost/生成proto；storage-kv的 `etcd_store.rs` 使用etcd-client/tonic。
它们分别把持久化codec和Etcd适配器与可复用存储契约放在同一crate。不能仅删除Cargo
依赖，否则现有生产实现无法编译；不能用新feature或只检查直接边掩盖传递依赖。
修复需要将后端中立存储契约与协议/Etcd适配器拆开，并调整placement、调度和存储调用方；
这是跨既有基础包的边界重构，超过156“仅补验收、禁止塞入大量新实现”的范围。
本次没有擅自进行该重构或放宽架构门槛，没有新增生产依赖。

[性能重放](logs/task_156/performance-regression.json)在相同冻结场景重新执行任务155，
得到36条新测量与666步leader闭环，最终 `[334,333,333]`。分数、改善量、操作数和rank
与155基准一致；耗时/分配量只记录，无自定阈值或Go耗时对比。
跨进程步骤**并非逐字节相同**：4节点region冷/热6条样本由原先移出store1变为移出
store3，因而少了TransferLeader。原始前后步骤完整保存，不把这种差异归一化成相同。
只读Go balance_region.go按分数排序且无ID tie-break；固定快照1/2/3副本负载相等，既有
任务155测试明确允许移出1..3，并按实际source校验kind与步骤。同一快照的冷/热步骤仍
严格一致，本次harness全部通过；不能将跨进程不同操作说成同一操作。
另以[performance-diagnostic](logs/task_156/performance-diagnostic.json)重复两个新进程，
均3项通过，各36条测量与666步闭环，保存全部原始输出，未修改155期望或生产排序。
[环境](logs/task_156/environment.json)注明未独占机器，数据仍是debug profile合成快照；
MemoryStore不是生产durability证明，不宣称线上TiKV吞吐量或生产后端替代通过。

## 红灯、最终验证与复现

[缺target红灯](logs/task_156/red-registration.json)退出101，只证明接线缺失。
[initial](logs/task_156/initial.json)记录上述decoder期望错误，2通过/1失败；
[corrected](logs/task_156/corrected.json)修正来源期望后3通过。
随后[red.py](logs/task_156/red.py)暂时在测试装配中污染恢复字节、取消暂停、禁用I/O
故障边界；同一3个测试均产生行为断言失败，Cargo退出101，0通过/3失败。
[变异快照](logs/task_156/mutated-test.rs.gz)和[红灯日志索引](logs/task_156/red.json)
可审查；不是生产算法缺失的证据。finally等待子进程完成后恢复源码，再执行全部最终验证。

[最终原始记录](logs/task_156/verified.json)保存每条确切命令、合并stdout/stderr、退出码、
日志SHA256、线程数与槽位；[数量汇总](logs/task_156/verified-summary.json)统计libtest实际结果。
所有最终验证使用默认features、CARGO_BUILD_JOBS=4；性能target单线程，其余RUST_TEST_THREADS=2。

复现入口（仓库根执行）：

```sh
python3 docs/rust/schedule-execution/logs/task_156/verify.py repeat \
  'cargo test -p astersql-pd-compat-tests --locked --test schedule_acceptance -- --nocapture'
```

runner按1–10原子mkdir动态领取共享槽位，写task/session/PID，所有Cargo及其子进程继承
绝对CARGO_TARGET_DIR；正常/失败/中断均等待子进程退出后只释放自己的锁，保留缓存。
本任务实际使用槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

[只读Go拦截器](logs/task_156/reuse-go/go)拒绝实际Go构建/测试，仅复用task030已核验
SHA256的既有etcd I/O fixture二进制与只读kvproto ZIP。调用记录见
[go-reuse](logs/task_156/go-reuse.jsonl)；不运行make、failpoint注入/启停或Go模块整理。
[audit.py](logs/task_156/audit.py)对照[开始快照](logs/task_156/baseline.json)复核所有
Go/模块文件（含ignored/untracked）的集合、内容和Git状态，两份plan保持不变；
[审计结果](logs/task_156/audit.json)同时核对日志哈希、测试覆盖、依赖与git diff --check。

## 最终检查结果与阻塞交接

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 无无关Rust格式差异 |
| `cargo test -p astersql-pd-compat-tests --locked --test schedule_acceptance -- --nocapture` | 0 | 3通过；54条工厂故事+9条非法输入/名称 |
| `cargo test -p astersql-pd-compat-tests --locked` | 0 | 32通过、1既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1924通过、0失败、1既有忽略tso_processes |
| `cargo tree --workspace --locked` | 0 | 树已保存，不能将命令成功当架构通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_058 -- --nocapture` | 0 | 8通过 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_067 -- --nocapture` | 0 | 3通过 |
| `cargo test -p astersql-pd-compat-tests --locked --test schedule_performance -- --nocapture` | 0 | 3通过，36测量+666步闭环 |
| `python3 docs/rust/schedule-execution/logs/task_156/audit.py` | 1 | 只读/覆盖/性能质量检查通过；75条协议/传输依赖路径阻塞 |

[审计执行记录](logs/task_156/audit-run.json)保存实际失败退出码和stdout/stderr哈希。
1983个Go/模块文件（含ignored/untracked）及两份plan的内容、集合和Git状态均保持不变。
自己的槽位锁已释放，缓存保留。任务58/67文件由主会话后续处理；未改写原始主题6。

不能将当前结果标为“已完成”或“已完成，待回归”：验证已实际运行，失败是明确的架构
验收缺口。解除阻塞需要先修复或由来源产物明确重新裁定上述存储适配层依赖边界，随后
重跑依赖审计及本任务最终验证；本次未自动修改来源或只读plan，也未删除156编号任务。

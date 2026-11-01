# 任务101：HTTP、配置和指标矩阵回归

状态：已完成（2026-11-02）。依赖[任务100](task-100.md)已核验；基准提交
`7c9efab1c3b6a452ec4f2e719d37addee730ded2`。

## 范围与清单

[执行目录](../../../tests/rust-compat/fixtures/task-101-catalog.json)逐字保存
[HTTP 清单](wire-inventory.md)和[配置、指标、flag 清单](surface-inventory.md)
的 759 个编号行、原 owner、运行入口与未完成依赖。没有修改清单或 plan.md。
未知编号、缺行、重复行、owner 改变、来源行改变、缺少运行入口/明确依赖均失败；
独立负向测试实际破坏这些条件。

| 类别 | 总行数 | classic 行 | 后续领域的边界回放 | 仅后续依赖 |
| --- | ---: | ---: | ---: | ---: |
| HTTP | 263 | 197 | 53 | 13 |
| 配置 | 247 | 190 | 0 | 57 |
| 指标 | 187 | 123 | 0 | 64 |
| flag | 62 | 18 | 0 | 44 |

528 个 classic 行、53 个边界行复用现有只读 supervisor。231 个行仍含未完成依赖，
其中53个仅证明边界响应；没有把它们计作对应领域实现。owner78的v2/GC/keyspace/
MCS边界、owner102的最终准入、DR/maintenance及其他主题8/9/10项保持未完成。
共享指标 M-037（maintenance）和 M-151–153（MCS keyspace group）按实际领域列为
主题8依赖，保留原始共享owner；没有把classic指标缺口转移给后续任务。

45 个清单入口加公共中间件入口去重后共51个supervisor，另加入真实 `cluster-metrics`
回放，共52个入口。
各既有场景保留完整 HTTP 状态、JSON、错误、重定向/认证/TLS/限流和持久状态断言。
原始响应来自既有业务场景，不复制领域实现或增加固定响应。

## 真实缺口和修复

初始测试已编译运行，真实 Go/Rust `pd-server` 均 ready，但 Rust 缺失六个
`pd_core_*` family；M-022/023四个启动锁样本断言失败，退出101。
后续真实 AllocID、TSO 和描述符检查继续约束实际接线，不能靠常量输出补齐。

- `pkg/core/metrics.rs` 接入 RegionsInfo 的真实锁获取、批量查询、心跳各阶段和
  bucket 结果。Rust仍使用同一个索引锁，按 root/subtree 操作分类测量，不伪造第二把锁。
  锁等待沿用15秒采样、首窗口跳过；心跳 breakdown 受当前配置控制。
- `pkg/utils/metricutil/classic_metrics.rs` 定义宿主拥有的classic collector；ID分配在
  实际持久窗口提交后更新，TSO在真实同步/更新/reset分支记录，lease使用真实单调
  TTL与请求结果，runner仅在实际完成后计成功，operator使用实际准入/步骤/归档结果。
  普通metadata读取不增加txn指标；事务错误及取消由真实事务边界计数。
- `pkg/server/service_metrics.rs` 将既有checker、filter、statistics、scheduler、scatter、
  hbstream、labeler、affinity、sync采集器装配到宿主Registry。临时Registry仅用于
  构造定义，采集器克隆注册到实际Registry，失败回滚和关闭注销均由宿主scope控制。
  domain handles不持有Registry或service，避免服务与term之间形成引用环。
- runtime复用已有worker：真实后端健康检查、store/region/hot统计、rule/label数量、
  affinity、hot pending、store config、region event及network slow观察均从当前状态采集。
  bootstrap唤醒现有store-config worker；scheduler status只看pause/halt，不把准入限额
  伪装为暂停状态。debug balance指标由真实solver投影值产生，并受debug开关控制。
- store观察使用弱成员注册表。持有成员读锁直到发布；删除持有写锁并清理采集器。
  不可变旧快照不能在删除后重建series；scope销毁不会注销宿主的其他collector。

## 指标差分的口径

目录为97个classic/shared collector保留Go来源的名称、HELP、类型和label名契约；
95个来自原15/86/101共享行，另含health与scheduling allowance。26个服务层指标行
继续由已有 `service-metrics` 场景验证。progress、bucket、health由实际worker按生命周期
注册，不要求冷启动时凭空产生业务样本。

family内部也保留明确的部分依赖：M-035的`type=keyspace-idAlloc`由主题8的keyspace
生命周期准入；M-078–082的`runner_name=log-async`是Go异步日志队列，Rust当前使用
既有日志sink，其异构队列观测归主题9。这六行的classic标签仍强制验收，后续标签
逐项记录在目录及原始抓取的`scope_dependencies`，不是整行忽略。验证器拒绝扩大
这些例外。M-154的`type`兼作实际后端URL，独立fixture的URL值原样保留；静态值
`endpoint`仍比较。scatter的两个internal CPU事件在Go包初始化即绑定，Rust保留相同
冷children，不把冷值当作发生过内部CPU scatter分支的证据。


[旁路观察器](../../../scripts/http-regression-oracle.py)仅检查该次supervisor的后代进程，
获取它们实际拥有的loopback监听端口，在业务回放期间抓取 `/metrics`。保留首份、末份
及每次新schema的原始exposition、状态、content-type、地址、进程命令和时间。
每个supervisor的原始运行/清理报告仍保留；observer不替代业务请求。

对实际到达的Go family，Rust必须有真实样本且HELP、TYPE、label名和histogram边界
相同；有限语义label的Go取值必须在Rust业务回放中出现。地址、成员名和各独立fixture
分配的ID不作跨进程相等断言，原始值仍留在exposition。Rust可能提前绑定冷counter，
所以不要求观测顺序或冷绑定集合完全相等。没有比较随机延迟或异构进程的累计总数。
静态登记和业务未到达的零值不能证明某个计数分支发生过。

专门的同场景测试另外验证实际值：重复/空/不存在的批量查询计数与histogram次数，
AllocID跨窗口后的gauge与对应真实etcd持久值一致，TSO失败/忽略较小值/成功reset的
精确计数增量。schema负向控制会删除family，改变HELP、TYPE、label名、有限label值
或bucket边界，必须失败。真实etcd暂停、停止和恢复测试验证事务失败、future取消、
成功次数与数据仍持久；没有用mock调用次数代替结果。

## 验证与运行证据

已核验前三批成功报告和日志 SHA；本次归档阶段未运行 Cargo。以下均为默认 features，
真实 etcd；普通包级/工作区命令不全局设置双实现环境，52个监督器各自对子进程设置
`PD_COMPAT_MODE=dual`、绝对路径 Go/Rust 二进制。工作区输出设置
`RUST_TEST_NOCAPTURE=1 RUST_TEST_THREADS=8 CARGO_BUILD_JOBS=4`。

| 阶段/报告（归档内文件名） | 确切命令 | exit | 通过/失败/忽略 |
| --- | --- | ---: | --- |
| final-focus | `cargo test --locked -p astersql-pd-compat-tests --lib task_101_ -- --nocapture` | 0 | 14/0/0 |
| followup-native | `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| followup-native | `cargo fmt --all -- --check` | 0 | 格式 |
| followup-native | `cargo check --workspace --locked` | 0 | 检查 |
| followup-native | `cargo test --workspace --locked --exclude astersql-pd-compat-tests` | 0 | 2190/0/78 |
| followup-package | `cargo check --locked -p astersql-pd-compat-tests` | 0 | 检查 |
| followup-package | `cargo test --locked -p astersql-pd-compat-tests --lib go_client_acceptance_test::task_099_behavior -- --exact --nocapture` | 0 | 1/0/0 |
| followup-package | `cargo test --locked -p astersql-pd-compat-tests -- --nocapture` | 0 | 55/0/1 |
| followup-workspace | `cargo test --workspace --locked` | 0 | 2245/0/79 |

final-focus及后两阶段均先执行
`cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`，退出0。
final-focus还执行 `cargo fmt --all`，退出0。实际槽位：聚焦1、原生1、包级2、工作区1；
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-N`（N为上述实际编号）。
报告确认自有进程结束及槽位释放，缓存保留。本次归档不领取槽位。

计数按顶层目标统计，包级和完整工作区各另有23个嵌套监督器测试通过，不能重复计入
55或2245。完整工作区为164个顶层目标；79个ignored是需要专用入口的显式测试，
不是通过证据。其逐项名称、目标和原因在报告与完整日志中保存；任务101的14个兼容测试
均实际运行，覆盖生命周期、负向控制和52个入口回放；工作区另覆盖真实事务失败/取消/恢复。

原生、包级验收后，批次3修正了三个真实异步fixture问题，并在提交
`9d9e9e257faeb4155315a5798387dc273aec4f62` 固定：region lookup等待发布时接受经过
header校验的正常调度命令；scheduler恢复等待每个实例ready再比较原配置；同地址
集群重启保留socket占用。最终完整工作区在这些修正上成功，源码指纹与交接一致。
旧 `final-workspace`、批次3中间失败及端口/磁盘故障仅作为失败历史保留，未列为通过。

task110原fixture的字面量推导遗漏三个 `opType.String()` hot事件；只读枚举Go声明
并解析对应符号后，精确集合为104。fixture增加三个真实声明及来源说明，原生预检的
`followup-native-fixture-audit.json`、原始scheduler失败与通过日志、最终工作区测试共同
覆盖此修正。它是静态声明集合校验，不是104个事件都动态发生的证明。

最终目录仍为759行、97个静态classic/shared collector契约；最新完整回放实际共同到达
51个family schema（包级运行50个，定时worker时序不同），逐项比较已到达的HELP、TYPE、
labels和bucket边界。静态97与动态51口径不同，未到达分支和上文231行后续依赖仍按
原边界记录；没有将其扩大为全部领域或每个label分支完成。

永久证据为 [task-101-evidence.json.gz](task-101-evidence.json.gz)，14,555,426字节，SHA-256：
`2d9467c41d165e033018f07a834fb94cbc1aca586ecf601c86a371195b080b07`。
其中512个原始文本文件、52份最新原始监督器报告和48,937条去重指标文本行；保留实际
command、exit、HTTP观察、原始清理报告、已有失败日志、最终源码与二进制指纹。
早期聚焦52份target日志及包级205个target产物在本阶段入口已不可用；没有将其冒充
已归档。现存历史tmp日志和清单仍完整保存，包级缺失路径及原SHA逐项写入
`final-audit.json`；最终工作区205个产物均实存且SHA一致，提供全部52份当前原始报告。

独立解压审计逐文件校验SHA，并用报告内 `$raw_lines` 索引按顺序拼接 `line_table`，
校验每段exposition的SHA；恢复所有JSON字段后以 `json.dumps(report, indent=2).encode()`
重建原始报告字节，与报告SHA和本机原始字节逐字一致，并核对command/exit。
归档包含审查后的 `archive.py`、`final-audit.py`，只执行证据处理，不调用Cargo。

最终 `frozen-source.json` 覆盖4544个Rust/JSON/Python/Cargo输入（含task110 fixture），
全部与最新工作区快照一致。`go-final.json` 按只读监督器相同snapshot规则遍历，
1101个Go/模块文件的存在性、SHA和Git状态与 `resume-go-before.json` 逐项相同。
两份只读plan哈希一致；本收尾plan SHA为
`368dc540a7bd5471d217890fd342705d698e11a72e466b555c8b9b85760942b0`。
最终Go二进制SHA为 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；
最终Rust二进制SHA为 `c3c59f68abf91af1a611fab3d8f85a28800fe386d5f342244763a3c5cc61c968`。
任务101范围内已无必要验证缺口；任务102最终覆盖审计未执行。

成员/leader监督器另外需要现有etcd与相邻etcdctl：本轮设置
`PD_ETCD_BIN=/tmp/pd-task043-etcd-v3.5.15/etcd`，复用任务43产物，`etcdctl version`
实际返回3.5.15/API3.5；没有下载或生成Go工具。运行入口在开始矩阵前检查此条件。

使用现有Go二进制、已有Go fixture源码的 `go build -mod=readonly`，以及固定kvproto
`f8ad079c8e9812ed5900b0c12b5f1d97171464c4` 的只读缓存。Go fixture关闭CGO、下载及
工具链自动获取；没有Go测试/failpoint/生成/格式化/依赖整理写入。
所有`.go`和Go模块文件（含ignored/untracked）开始/结束的存在性、SHA-256和Git状态
必须完全一致。保留用户已有任务94–100删除和prompt修改；plan.md SHA-256固定为
`a59eba65612e814ada50274525cbb41a81df792f2d2966c2e60a2deae5d84615`。

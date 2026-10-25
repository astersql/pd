# 调度执行内核：依赖与只读取证准入

状态：已完成。批次1通过准入；来源固定为
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`。
本结论只开放后续计划的实施，不代表调度领域已迁移或全部行为已有运行 fixture。

## 来源与行为归属

[source-inventory.json](source-inventory.json) 固定14个包的85个生产文件、
1213个公开声明/成员、900个内部声明、576个测试/benchmark/生命周期声明，
以及18类工厂、25个 failpoint、44个指标构造器、24个存储调用候选和17个
goroutine/timer/runner 位置。每个声明和位置带源文件、行号及编号任务归属。
`file_tasks` 是来源覆盖表；各项 `tasks` 是行为/支持代码的实施责任，不能当成运行覆盖。
接口嵌入、公开字段与私有类型的公开方法也单独列出；576不是通过测试数量。

清单同时保存生产直接 imports、全仓库反向消费位置（区分测试）、相邻测试名称、
指标 namespace/subsystem/name/help/label 与原构造表达式，以及存储 key helper 的
只读定义。`storage_call_candidate` 还需在所属行为中确认具体后端；不将普通 Load
调用自动解释为持久写入。配置 key 复用 keypath/ConfigStorage；operator 队列和
终态记录的内存语义不能被解释为在途操作已经持久化恢复。

18类工厂逐项保存在 `scheduler_factories`；注册归任务106，调度、配置及传输行为
分别归其列出的编号任务。Go plugin ABI 归任务117的明确不支持边界。

## 真实依赖

- 主题4依据 [规则链联合验收](../schedule/integration/41-证据.md)：四项规则链测试、
  45次 fit 回放，历史 workspace 967通过、0失败、1既有忽略。
- 主题5依据 [状态面联合验收](../state/task-53.md)：六项主题测试，历史 workspace
  1312通过、0失败、1既有忽略；包含真实 heartbeat/split/merge/stale、断连追赶和重启。
- 已核验 config/provider、core/BasicCluster、keyrange/Manager、labeler、placement、
  affinity、statistics/HotStat/StoresStats/RegionStatistics/BucketWorker、id/Allocator
  的公开 Rust 声明及文件哈希。详见清单的 `verified_dependency_api_sources`。
  这些是可复用依赖；本次没有把 crate 存在当成全包迁移证明，也没有重跑历史 Cargo 回归。
- `server/cluster` 已有 StatePipeline、submit_region_heartbeat、heartbeat effects 和
  overlap 统计/KV/history 清理，后续 facade 必须复用宿主所有权及 runners。
  `astersql-pd-cluster` 属于 server/cluster；pkg/cluster 采用计划规定的 cluster-utils。
  新调度接口适配现有领域对象，不复制状态面，不把 Go import 图照搬到 Rust。

主题4/5来源 Go 相对各自冻结基准无差异；起点1101项源码/模块哈希还与 task47/53
的完整 before 清单逐项相同。根 Cargo.toml、Cargo.lock、rustfmt.toml 只读。
本任务没有 Cargo 调用、生产修改或新依赖；所有后续 Rust fixture 仍须先取得运行期望。

## 四项阻塞的实际解除证据

完整命令、二进制 SHA256、有效测试名称、原始 stdout/stderr 与退出码保存在
[manifest.json](logs/task_001/manifest.json)、[原始输出](logs/task_001/outputs.json)
和 [oracle-index.json](oracle-index.json)。展示版日志只去 ANSI、展开 tab、清理行尾空白，
原始内容保存在 outputs.json 中，没有删去失败尝试或 Go 错误日志。

| 场景 | 实际取证方式与结果 |
| --- | --- |
| 派发时并发替换 | [operator.out](logs/task_001/operator.out)：原 TestConcurrentRemoveOperator，1项通过。Dispatch 已持有旧 op 后，Delve 调用阻塞 FIFO，50ms 后另一原测试 goroutine 替换；500ms 后继续，最终仍为新 op。没有修改对象或断言。 |
| scatter 失败与记账 | [scatter.out](logs/task_001/scatter.out)：原 TestScatterInternalPlanningFailureAccounting 的 seeded=false/true 两项通过。原测试回调共调用12次；每次选择错误、错误分类、nil operator、记账不变和回调次数均由原断言检查。 |
| 慢网络持久化失败/重试 | [persist.out](logs/task_001/persist.out)：原 TestAddNetworkSlowStoreLockedSkipsGaugeOnPersistFailure，1项通过。只在 baseSchedulerConfig.save 的错误输入边界设置一次非 nil error，实际回滚暂停和配置、不发布 gauge；移除该控制后原测试重试成功并发现 gauge。 |
| 原生进度 GC | [native-events.json](logs/task_001/native-events.json)：隔离原 Go 服务、原 Rust 协议探针和 curl。store1 Offline→Up，三项 removing 指标在完成后及15秒后保留，在启动127.988秒时全部删除；之后服务仍响应请求，最终 TERM/join=0。未改变30秒过期/2分钟GC周期。 |

前三项只读编译现有 Go 测试为 Linux/ARM64 调试二进制：
`GOOS=linux GOARCH=arm64 CGO_ENABLED=0 GOFLAGS=-mod=readonly GOPROXY=off`
配合 `go test -c -gcflags='all=-N -l'`。编译本身不算测试。
没有运行 failpoint-ctl/toolexec、源码转换、Go 生成/格式化、模块整理或 make。
不能把这些有明确调试控制的运行描述成“未注入就裸跑依赖 failpoint 的测试”。

Scatter 通过 Delve 读取原测试已注册的 reflect.Value，调用原 Go 回调和原 args；
没有新建回调或替换算法。原 marker 的空实现让编译器把 peer 地址留在栈上，
Delve 因此要求 `call -unsafe`。已审查该回调只同步解引用该地址、不保留地址；
该选项仅用于这次调试调用的保守逃逸检查，不关闭 Go、测试或仓库检查。
具体12次控制见 [scatter-full.commands](logs/task_001/scatter-full.commands)。

持久化场景未绕过返回错误后的代码，只设置现有局部 `err`，继续执行真实 save 返回、
persistLocked 回滚和 scheduler 逻辑。所用 error 文本为原故障的 `fail to persist`；
没有将该实验声称为真实磁盘故障、错误具体实现类型或 wire 文本等价证明。
直接调用 marker 的闭包曾被 Delve 的模块数据解码错误阻止；失败日志保留，
最终取证改在原调用方的错误输入边界，命令见 [persist-frame.commands](logs/task_001/persist-frame.commands)。

原生服务 SHA256 与 task47 相同：
`446a9ce18ed5ac513d4cc66306cfe276db44cd2198db2d9e4e53d20520040523`。
它的构建元数据是 fc993926…+dirty，不能冒称由当前 HEAD 干净构建；
但对应全部 Go/模块文件的 before 清单与当前冻结源码逐项一致。
Rust 探针只发送原有 protobuf 请求并观察响应，不生成 Go，不产生 Rust 实现期望。
服务使用临时端口和独立 WAL，所有自有服务/容器退出，临时 WAL 删除。

额外观察：本次 native 配置的 `schedulers=[]` 后，Go 仍装配了默认工厂，
create evict-slow-store 返回 scheduler existed。不能仅凭空配置数组声称运行时无调度器；
GC 取证依据实际进度/指标、仍可用的服务及原生清理周期，不依赖这个假设。
这不改写既有 state 运行输出，也不把它扩大为调度决策验收。

## Oracle 可用性及后续约束

索引严格区分实际运行结果、源码断言和未冻结输出。四项曾阻塞的只读取证入口已
实测，当前没有未解决的必要准入取证障碍；无需受限 CI 日志或来源不匹配的产物。
公开行为尚未逐项运行的 fixture 明列为 pending，所属任务必须在实现之前冻结原始
输入/输出及归一化规则，不能从 Rust 结果反推期望、把本准入当成调度迁移完成，
或把 candidate 命令当成允许裸跑整个包。

原候选14包 `go test ... -count=1` 不获准运行，因为它包含未做逐路径补偿的
failpoint 测试。已执行的普通 Go allowlist 仅为四个 progress 数值/窗口测试和四个
scheduler callback/Prepare 回滚测试；本次重新保存完整输出，见
[safe-tests.json](logs/task_001/safe-tests.json)。二者各4项通过。
对需要控制的故障采用上述精确命令，不允许把一个控制泛化为全包无条件放行。
classic/default 为本次实际模式；nextgen 的逐行为运行仍是所属任务的义务。

历史 statistics 三项热点影响量 Go 输出也已入索引，其用途限定为数值影响量，
不能替代18类 scheduler 的策略结果。没有使用上游成功徽章、不可访问日志或
有源码漂移的旧输出补足当前测试。

## 验证与交接

`python3 verify.py` 重查来源/公开声明归属、18工厂、证据哈希、原测试真实退出、
原生GC观测、1101项源码/模块及包含 ignored 构建副本的3012项文件、17份 plan.md，
并核对根 Cargo 文件和链接。两份 JSON 均通过 `python3 -m json.tool`；
`git diff --check` 通过。结果见 [verification.json](logs/task_001/verification.json)。
文档任务没有调用 Cargo、启用 feature 或修改 Go；不以零测试、忽略测试或编译成功
替代上述运行证据。只提交本目录证据，保留其他未跟踪计划。

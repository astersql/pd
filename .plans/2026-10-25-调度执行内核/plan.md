# 调度执行内核计划

目标：将来源主题 6 拆为可在单个会话实现和验证的包级、行为级任务，完成整个 Rust 调度链。

范围：schedule/core、filter、operator、hbstream、checker、scatter、splitter、全部 18 类内置 scheduler、handler、diagnostic、调度 facade、pkg/cluster、progress、operatorutil，以及必要的心跳与管理协议适配和主题验收。

范围外：Go 文件与模块改写、Go plugin ABI、新插件路线、主题 7 的完整 server/api 路由装配、生产元数据后端选型。

假设：主题 4/5 提供已验证 Rust API；当前 workspace 的已有 crate 仍需核实覆盖。来源状态为未开始，源码基准为 `836fdc7cf3ef34f7eabc9d051418995bb97efabe`。准入失败必须记录，不将依赖存在视为可用。

## 设计决策

- 选按包建 canonical crate、按行为拆任务；整目录翻译过大，集中单 crate 容易形成依赖环。
- 保留来源顺序与 Go 只读边界；领域命令独立于 wire；全部任务串行避免入口冲突。

## 架构说明

- 基础状态/规则 → core/filter → hbstream/operator → checker/scatter/splitter → schedulers → facade/管理/集群桥接。
- 复用宿主 Tokio；proto 仅在适配层；不复制状态面。

## 来源与只读约束

来源：[主题 6](/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-22-pd-rust-渐进迁移主题/6-迁移调度执行内核.md)；上层架构：[主题指南](/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-22-pd-rust-渐进迁移主题/plan.md)；Rust 边界：[ADR-001](/Users/Shared/work/dir/data/codes/pd/docs/rust/architecture.md)、[移植指南](/Users/Shared/work/dir/data/codes/pd/docs/rust/porting-guide.md)。这份计划及全部子计划的 `plan.md` 生成后只读，状态只记录在编号任务文件。原主题文件保持原样，子任务通过不代表主题通过，只有主题验收证据完整后才能按来源规则完成主题。

- Go 仅供只读参考。禁止新增、修改、删除、重命名任何 `.go` 文件（含生产代码、测试、oracle、生成文件及临时目录中的 Go 工具）；`go.mod`、`go.sum`、`go.work`、`go.work.sum` 同样只读。所有新增实现与测试使用 Rust，必要的非 Go 验证脚本和证据文档须在任务范围内。
- 禁止通过生成器、格式化、依赖整理、failpoint 注入或构建脚本间接改写 Go；不得运行会改写 Go 的 make 测试入口、failpoint 启停命令、`go generate`、`go mod tidy` 或 Go 格式化写入命令。不得以“只增加测试/工具”“临时修改后还原”或扩大任务范围绕过约束。
- Go oracle 仅复用已有只读源码、测试、fixtures、已保存输出或已有二进制；运行已有 Go 工具/测试前，必须确认整个调用链不写上述文件且不需要 failpoint 注入。需要 failpoint 的测试不得直接裸跑来替代验证；无法只读取得必要运行证据时，记录具体缺口并按任务阻塞规则处理，不降低验收标准，不将源码推导冒充运行差分。
- 开始与结束时核对上述文件的 Git 状态及内容，保留用户已有变更；本任务不得产生任何 Go 文件或 Go 模块文件差异。本约束优先于计划、执行技能或验证要求中任何相冲突的写入指令。

## 已核实的工程边界

- 前置主题 5 已有 `docs/rust/state/task-53.md` 联合差分证据，可审查 `task47` 只读原生 Go 二进制及既有探针；这不代表已取得调度领域全部 oracle。准入必须核验其当前可用性与基准差异。
- 根 workspace 已启用 Rust 2024/resolver 3；`rustfmt.toml` 使用 edition/style_edition 2024；已有 Cargo.lock。
- 新包按相邻 Go 目录新增 Cargo.toml/lib.rs；集中声明 workspace dependencies，加入 members；不为行为任务各建 crate。
- `server/cluster` 已占用 `astersql-pd-cluster`；`pkg/cluster` 使用 `astersql-pd-cluster-utils`，两者职责分别是状态管线与调度桥接。
- 新 Rust 文件以 AsterSQL 2026 版权标记开头，测试独立 `*_test.rs` 并显式注册。可用生产文件同步写同名 `.rs.md`，沿用相邻文件模式，不引入当前不存在的 docs manifest。
- 新任务首次接入新 crate 时允许 `cargo generate-lockfile --offline` 更新 workspace 路径包；审查 Cargo.lock 中远程依赖版本不应无故变化，之后所有检查使用 `--locked`。没有离线依赖时记录实际错误，禁止临时移除锁定要求。
- 不添加 feature 开关；全部任务使用各 crate 默认 features。不能将 Go build tag 当成已存在的 Cargo feature；classic/nextgen 分支按当前 Rust API 可表示输入覆盖，缺失依赖明确报告。
- crate 与文件路径在编号任务中标为“新建/拟建”者是本计划的实施定义，不表示现在存在。最早使用它的任务负责创建和接线。

## 证据格式

运行 fixture 至少记录 source_commit、来源二进制或输出哈希、原始 inputs/outputs、适用 mode、逐字段 normalization 和实际命令；测试期望不能从待测 Rust 实现反推。每个任务自己的证据文件记录实际测试名称与案例数，保留 stdout/stderr/exit code。不存在可审核 Go 运行结果的用例明确阻塞，不能填造输出。

## 包级指南

- [调度迁移准入](00-准入/plan.md)：`pkg/schedule`；crate `不适用，纯文档准入`。
- [调度核心契约](01-调度核心契约/plan.md)：`pkg/schedule/core`；crate `astersql-pd-schedule-core`。
- [调度过滤器](02-过滤器/plan.md)：`pkg/schedule/filter`；crate `astersql-pd-schedule-filter`。
- [心跳命令与流](03-心跳命令/plan.md)：`pkg/schedule/hbstream`；crate `astersql-pd-schedule-hbstream`。
- [调度操作器](04-操作器/plan.md)：`pkg/schedule/operator`；crate `astersql-pd-schedule-operator`。
- [调度检查器](05-检查器/plan.md)：`pkg/schedule/checker`；crate `astersql-pd-schedule-checker`。
- [区域散射](06-区域散射/plan.md)：`pkg/schedule/scatter`；crate `astersql-pd-schedule-scatter`。
- [区域拆分](07-区域拆分/plan.md)：`pkg/schedule/splitter`；crate `astersql-pd-schedule-splitter`。
- [内置调度器](08-内置调度器/plan.md)：`pkg/schedule/schedulers`；crate `astersql-pd-schedule-schedulers`。
- [调度诊断管理](09-诊断管理/plan.md)：`pkg/schedule/diagnostic`；crate `astersql-pd-schedule-diagnostic`。
- [调度协调门面](10-调度门面/plan.md)：`pkg/schedule`；crate `astersql-pd-schedule`。
- [调度管理处理](11-调度管理/plan.md)：`pkg/schedule/handler`；crate `astersql-pd-schedule-handler`。
- [集群调度桥接](12-集群调度桥接/plan.md)：`pkg/cluster`；crate `astersql-pd-cluster-utils`。
- [调度进度追踪](13-进度追踪/plan.md)：`pkg/progress`；crate `astersql-pd-progress`。
- [操作断言工具](14-操作断言工具/plan.md)：`pkg/utils/operatorutil`；crate `astersql-pd-operatorutil`。
- [调度主题验收](15-主题验收/plan.md)：`tests/rust-compat`；crate `astersql-pd-compat-tests`。

## 开发策略

- 对会改变行为的代码，使用失败验证—通过验证—重构；先保存 Go 运行 oracle 与 Rust 失败证据。
- 实现只覆盖失败测试所证明的行为，使用真实 Rust core/状态/规则/操作对象；慢外部 I/O、时钟与随机输入才允许在边界控制。
- 源码推导只作测试设计依据；必要运行 oracle 缺失时报告证据缺口，不把源代码分析冒充运行差分，也不能用零测试或忽略测试完成任务。
- 每个任务同时验证正常、边界、错误及其范围内故障；重点比较步骤、kind、优先级、目标、影响量、confver、diagnostic、持久化和 heartbeat 响应。
- 序号就是线性执行顺序，每个序号独占同编号批次；包内和包间均不并行。Cargo 分槽只管理构建缓存，不解除 lib.rs、Cargo.lock、全 workspace 格式化或差分环境互斥。
- 根 prompt.md 覆盖所有任务，子 prompt.md 仅覆盖该包；两者是同一批任务的两种入口，不能重复启动。删除已完成任务后，从提交与证据确认依赖，不能仅以“文件不存在”认定完成。
- 任务进入完成状态后按 `$git-commit` 只提交实际相关变更；单独状态更新或删除不提交。受无关环境影响时可记录已完成，待回归，但它不满足后继任务真实依赖和主题完成门槛。

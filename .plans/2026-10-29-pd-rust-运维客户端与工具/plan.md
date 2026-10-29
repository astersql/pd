# PD Rust 运维客户端与工具计划

目标：将主题九拆为单会话可验证的小任务，补齐运维能力、所需 Rust 客户端及工具的兼容证据。

范围：审计、维护指标、计量及存储适配、资源观测、Go GC 边界、版本、客户端、pd-ctl、备份恢复、模拟器、导出诊断与基准工具、Swagger、Dashboard 服务端与发行验证。子系统分别以明确的 canonical crate 和证据交接；此目录为来源史诗的联合拆分。

范围外：Go 文件变更、前端重写、Go runtime 嵌入、旧 Go 插件 ABI、原生持久化后端，以及没有真实消费者的 convenience API。

假设：基准 `87cce7d5f6a038fe16087ea01e613959c91ceeef` 已有 Cargo workspace；metering/config 与 versioninfo/kerneltype 已有 Rust 实现，其余能力仍须逐项核验。主题八和未提交改动不视为完成证据。

## 设计决策

- 比较整包翻译、工具先行、按可观察行为拆分；采用第三种，复用 canonical crate。
- Go GC 和外部 Dashboard 后端先核实边界；未确认替代方案不得进入实现。

## 架构说明

- 领域库 → 传输适配 → runtime → 薄入口；client 独立消费协议。宿主持有取消、Tokio handle 与 Registry。
- 工具不反向阻塞核心服务；MetaStore 保持外部 etcd 过渡边界。

## 开发策略

- 每个行为先取得失败测试，再最小实现、通过测试及重构；测试独立在 `*_test.rs` 并显式注册。
- 源码推导与运行 oracle 分列；真实副作用、错误与取消都需新证据。编译或零测试不能验收。
- 所有命令从仓库根执行，默认 features；新增 manifest、模块与测试名在任务中明确是拟建契约。已存在实现则原位补差距，禁止覆写其他会话变更。
- 新 crate 首次行为任务负责 manifest、workspace 注册和锁文件；不预建空 crate。依赖版本集中选择并锁定，不虚构版本或 features。
- 任务采用串行批次，线性顺序及批次明细见 prompt.md。公共 manifest、命令注册、runtime、全仓 fmt 与差分环境存在交叉写入，不能仅靠 Cargo 分槽并行。
- 独立端口及临时数据目录由测试动态创建，进程退出后清理；危险工具只用一次性集群。Cargo 编译缓存只用仓库共享槽位。
- 完成证据保存在 `docs/rust/ops-tools/task-NNN.md`，任务状态仅记录在对应编号文件；plan.md 生成后只读。此计划不实现代码，不更新来源史诗状态。
- 每项 Rust 实现遵守版权、领域边界与 library-first 约束；源码旁说明与文档索引若现有仓库要求则随该项更新，禁止伪造完整覆盖。

## Go 只读硬约束

- Go 仅供只读参考。禁止新增、修改、删除、重命名任何 `.go` 文件（含生产代码、测试、oracle、生成文件及临时目录中的 Go 工具）；`go.mod`、`go.sum`、`go.work`、`go.work.sum` 同样只读。所有新增实现与测试使用 Rust，必要的非 Go 验证脚本和证据文档须在任务范围内。
- 禁止通过生成器、格式化、依赖整理、failpoint 注入或构建脚本间接改写 Go；不得运行会改写 Go 的 make 测试入口、failpoint 启停命令、`go generate`、`go mod tidy` 或 Go 格式化写入命令。不得以“只增加测试/工具”“临时修改后还原”或扩大任务范围绕过约束。
- Go oracle 仅复用已有只读源码、测试、fixtures、已保存输出或已有二进制；运行已有 Go 工具/测试前，必须确认整个调用链不写上述文件且不需要 failpoint 注入。需要 failpoint 的测试不得直接裸跑来替代验证；无法只读取得必要运行证据时，记录具体缺口并按任务阻塞规则处理，不降低验收标准，不将源码推导冒充运行差分。
- 开始与结束时核对上述文件的 Git 状态及内容，保留用户已有变更；本任务不得产生任何 Go 文件或 Go 模块文件差异。本约束优先于计划、执行技能或验证要求中任何相冲突的写入指令。

## 参考

- 来源：`.plans/2026-10-22-pd-rust-渐进迁移主题/9-迁移运维客户端与工具.md`。
- 架构：`docs/rust/architecture.md`、`docs/rust/dependencies.md`、`docs/rust/porting-guide.md`。
- 已有实现：`pkg/metering/config/Cargo.toml`、`pkg/versioninfo/kerneltype/Cargo.toml`、`pkg/runtime/node.rs`、`pkg/server/lib.rs`。
- 验证入口：`tests/rust-compat/readonly_oracle.rs`、`scripts/run-pd-compat-readonly.sh`。执行前检查最新脚本及依赖，不能因名称带 readonly 就假定安全。

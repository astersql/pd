# 统计工具计划

目标：迁移统计维度与多维 TopN，提供可确定验证的基础集合。

范围：kind、constant、labels、TopN 更新、TTL；crate astersql-pd-statistics-utils。

范围外：core/bucket/热点算法、服务接线。

假设：pkg/statistics/utils 目前只有 Go；Rust workspace 已存在。

## 设计决策

- 比较整包翻译与按行为增量，选择逐行为闭环，降低单会话上下文负担。
- 比较协议进入领域与边界适配，选择领域独立；比较并行接线与串行，选择串行消除共享写入冲突。

## 架构说明

- core/storage/规则 → statistics 与 syncer → cluster → server 适配；不引入调度决策反向依赖。
- MetaStore revision 与历史索引独立，宿主持有时钟、Registry 和运行时。

统一执行入口：[全部任务提示词](../2026-10-24-pd-rust-统计同步与心跳状态/prompt.md)。本目录 prompt.md 仅摘录本包任务；全局前置依赖仍须满足。

## 固定边界

- 来源任务：`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-22-pd-rust-渐进迁移主题/5-迁移统计同步与心跳状态.md`；来源任务及其 plan.md 保持原样，不因拆分标记完成。源码基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，执行时核对后续差异。
- 前置为主题 4 所需公共能力已通过验收；当前 core、movingaverage、storage/kv、storage/endpoint、schedule/config、placement 已有 crate，但存在 crate 不代表所需能力已完整。缺口不得用桩替代，不新建平行实现。
- 检查根 AGENTS.md、.github/copilot-instructions.md 与目标目录更深规则。库保持 library-first，领域不依赖 protobuf/tonic；协议转换在 pkg/server。Go FFI、全局订阅器和私有 Tokio runtime 禁止。
- 每个生产 Rust 文件增加 `// Copyright 2026 AsterSQL.` 与相邻 `.rs.md` 行为说明；测试独立 `*_test.rs` 且显式登记。features 使用默认集合，不增加猜测开关。
- 生产依赖集中 workspace.dependencies，新 canonical crate 才登记 member/dependency 并更新 Cargo.lock；已有 crate 直接扩展。首次新增依赖先更新锁文件，再用 --locked 验证；不得绕过锁文件。
- 执行顺序为统一 prompt.md 的全局编号升序；每个批次仅一个任务，依赖所有更早批次，无同批并行。manifest、Cargo.lock、lib.rs、workspace fmt、Git 提交及共享差分资源均须串行；Cargo 分槽不改变源码和资源互斥。
- 先记录 Go oracle 输入/输出来源和失败证据，再实现 Rust。源码推导与运行差分分开标记，缺少必要只读运行证据时不得标记完成或将其归咎无关环境而降级验收。
- `pkg/syncer` 实际使用 SyncRegions/history index，不是 etcd watch。MetaStore revision 恢复是来源要求的独立边界；禁止混用两种索引，禁止以 meta watch 替代同步流。
- 真实 Etcd 测试复用 `pkg/storage/kv/etcd_fixture_test.rs` 的已存在夹具前先审查其 Go build 调用链与 `pkg/proto/build.rs`，确认只读源码/模块且不需 failpoint；产物遵循 Cargo 槽位。无法满足则使用已存在外部 etcd 实例与只读已有二进制，仍无法取得必要证据则阻塞，不能新增 Go 工具。

## Go 只读硬约束

- Go 仅供只读参考。禁止新增、修改、删除、重命名任何 `.go` 文件（含生产代码、测试、oracle、生成文件及临时目录中的 Go 工具）；`go.mod`、`go.sum`、`go.work`、`go.work.sum` 同样只读。所有新增实现与测试使用 Rust，必要的非 Go 验证脚本和证据文档须在任务范围内。
- 禁止通过生成器、格式化、依赖整理、failpoint 注入或构建脚本间接改写 Go；不得运行会改写 Go 的 make 测试入口、failpoint 启停命令、`go generate`、`go mod tidy` 或 Go 格式化写入命令。不得以“只增加测试/工具”“临时修改后还原”或扩大任务范围绕过约束。
- Go oracle 仅复用已有只读源码、测试、fixtures、已保存输出或已有二进制；运行已有 Go 工具/测试前，必须确认整个调用链不写上述文件且不需要 failpoint 注入。需要 failpoint 的测试不得直接裸跑来替代验证；无法只读取得必要运行证据时，记录具体缺口并按任务阻塞规则处理，不降低验收标准，不将源码推导冒充运行差分。
- 开始与结束时核对上述文件的 Git 状态及内容，保留用户已有变更；本任务不得产生任何 Go 文件或 Go 模块文件差异。本约束优先于计划、执行技能或验证要求中任何相冲突的写入指令。

## 开发策略

- 用失败验证、最小实现、通过验证和保持通过的重构闭环。已有实现通过的部分记录复用证据，只对真实缺口补失败测试；找不到缺口时不得为了红灯破坏代码。
- 每个任务的测试过滤器必须实际运行大于零个行为测试；编译报缺符号仅为第一步，补最小接口后还须看到真实行为断言失败。
- 复用真实 core、movingaverage、MemoryStore 和 EtcdMetaStore；仅在网络、时钟和后端故障边界控制时序，不模拟热点或放置计算结果。
- 完成证据先保存对应任务的证据文档，再依提交规则处理实际变更；只在对应编号文件记录阻塞，所有 plan.md 生成后只读。

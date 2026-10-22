# PD Rust 渐进迁移主题计划

目标：在已完成 Rust 基础纵切之上，按依赖由小到大迁移 PD，并为后续逐包拆分计划提供稳定的主题顺序与准入门槛。

范围：覆盖生产 Go 包到 Rust 的长期迁移分层、包级候选顺序、主题依赖、兼容性门槛和切换原则。现有 `proto/storage/member/id/tso/server/runtime` 作为基线继续演进；每个具体包后续另建计划目录并拆分需求。

范围外：本计划不直接实现任何包，不一次性拆解所有包的函数级任务，不迁移 `pkg/mock` 等 Go 测试夹具，也不重新开启已否决的原生 Raft 元数据原型。

假设：提交 `ea1328071` 至 `5a07ba0bd` 已建立可运行的 Rust 基础纵切，`6c19604e8` 已否决当前原生 Raft 原型；Go 实现继续作为行为 oracle，外部 etcd 仍是过渡元数据后端。

## 设计决策

- 比较过“按目录顺序”“仅按直接 import 数量”和“依赖层次加纵向兼容门”三种路线；采用第三种。import 数量只用于发现候选，不能替代领域边界、运行时风险和协议兼容判断。
- 使用动态就绪队列：只有 Rust 侧直接依赖已稳定、Go 行为已固定、差分入口可运行的包才可创建实施计划；同一主题内可跳过尚未就绪的包。
- 一个稳定 Go package 对应一个 canonical Rust crate；过小且只属于父包内部的目录是否合并，必须在对应包级计划中依据公开 API 和反向依赖证据决定，不能在本主题计划中预设聚合。
- 每个包单独建立 `.plans/YYYY-MM-DD-pd-rust-<包名>/`，先拆行为和失败验证，再实施；本计划只读，不记录包级进度。
- 本目录的编号任务是十个迁移史诗，用来跟踪依赖层和主题验收，不允许直接作为单会话编码任务；执行某个史诗时，先按其中的包顺序建立包级子计划，再逐包实施和提交。
- `pkg/mock`、测试工具和兼容夹具随真实行为测试按需重建，不作为生产迁移主题；客户端、命令行与 Dashboard 不阻塞服务端核心切换。
- 原生元数据后端保持独立决策线。当前先在 `MetaStore` 之上推进功能迁移，生产去 etcd 化必须等待新的 durability/共识方案和 ADR。

## 架构说明

- 保持 `domain/core <- storage/election <- statistics/schedule <- cluster/server <- runtime/bin` 的单向依赖；`proto` 只进入适配层，领域类型不暴露 prost 类型。
- 已有 `storage/member/id/tso` 是基础能力，不等于对应 Go 包全部完成。后续计划必须先做覆盖差距清单，扩展现有 canonical crate，禁止新建平行实现。
- 每个包以 Go 源码、Go 测试、调用方和线上可观察结果组成 oracle；通过 Rust 行为测试、Go/Rust 差分和故障场景后，才允许上层依赖。
- 迁移顺序优先降低后续返工：先稳定值对象、算法与契约，再迁移状态容器，之后才是调度、服务装配、管理面和发布切换。

## 主题依赖图

```text
已完成基础纵切
  -> 主题 1 叶子类型与通用算法
  -> 主题 2 核心领域模型
  -> 主题 3 存储端点、选主与配置契约
  -> 主题 4 调度基础契约与规则
  -> 主题 5 集群状态、统计与心跳
  -> 主题 6 调度执行内核
  -> 主题 7 PD 服务端与 API 完整性
  -> 主题 8 独立控制面服务与 MCS
  -> 主题 9 运维、客户端与工具
  -> 主题 10 生产元数据后端与发布切换
```

主题之间默认线性依赖；主题内部只有不写同一 crate、没有直接依赖且不争用同一差分环境的包级计划才能并行。允许较高主题中的独立包提前进入就绪队列，但不得绕过它的真实依赖。

编号任务与十个主题一一对应，批次 1 至批次 10 形成默认主线。某个史诗只有在范围内所有包级子计划均完成、主题级回归通过后才标记完成；包级状态不得写回本 `plan.md`。

## 迁移主题与候选顺序

| 主题 | 由小到大的包级候选 | 进入下一主题的证据 |
| --- | --- | --- |
| 1. 叶子类型与通用算法 | `pkg/core/constant`、`pkg/keyspace/constant`、`pkg/mcs/utils/constant`、`pkg/slice`、`pkg/codec`、`pkg/balancer`、`pkg/movingaverage`、`pkg/window`、`pkg/btree`、`pkg/ratelimit`、`pkg/cache`、`pkg/core/rangetree`，随后是被广泛引用的 `pkg/errs` 与纯值/同步类 `pkg/utils/*` 契约 | 值语义、边界条件、并发语义和错误分类与 Go oracle 一致；没有为了复刻 Go 工具包而形成杂物 crate |
| 2. 核心领域模型 | `pkg/core/storelimit`，再按 Store、Region、BasicCluster 等公开模型拆解 `pkg/core`；`pkg/response` 只在领域错误和协议转换边界稳定后进入 | 领域模型不依赖 proto/transport/storage；clone、range、epoch、store state 与限流行为有差分证据 |
| 3. 存储端点、选主与配置契约 | 扩展现有 `pkg/storage`，依次覆盖 `pkg/storage/kv`、`pkg/storage/endpoint` 与存储 facade；其后迁移 `pkg/election`、必要的 `pkg/utils/{keypath,etcdutil,configutil}` 和 `server/config` 的纯配置契约 | 所有持久状态都有 MetaStore 映射、revision/watch/lease/fencing 契约；配置默认值和序列化兼容，不把 etcd 类型泄漏到上层 |
| 4. 调度基础契约与规则 | `pkg/schedule/{types,keyrange,rangelist,plan,config}` -> `labeler/placement/affinity` | 配置、range、label rule、placement fit 与 affinity 通过差分；基础层不依赖 statistics/operator/schedulers |
| 5. 集群状态、统计与心跳 | `pkg/statistics/utils`、`pkg/statistics/buckets`、`pkg/statistics`，再迁移 `pkg/syncer` 以及不依赖调度决策的 Region/Store heartbeat 状态容器 | 重放 heartbeat 能得到等价 region tree、store 状态、热点与统计；乱序、重复、split/merge 和重启恢复均有 oracle |
| 6. 调度执行内核 | `pkg/schedule/core` -> `filter` -> `operator/hbstream` -> `checker/scatter/splitter` -> `schedulers` -> `handler/diagnostic` 与 facade，最后接入 `pkg/cluster`、`pkg/progress` 和 operator 工具 | 每类 operator/checker/scheduler 分别有确定性输入输出、持久化恢复和故障差分；不以 mock 调用次数代替调度结果 |
| 7. PD 服务端与 API 完整性 | 扩展现有 `pkg/server`/`pkg/runtime`，迁移 `server/cluster`、剩余 PD gRPC、`server/api`、`server/apiv2`、`server/join`、`pkg/basicserver`、传输类 utils、认证/TLS/限流/审计和完整配置启动路径 | Go client 可对 Rust 服务运行核心集成场景；gRPC/HTTP/metrics/CLI、滚动重启和优雅关闭兼容 |
| 8. 独立控制面服务与 MCS | 先共用 `pkg/mcs/{utils,registry,discovery,server}`；随后按 `metastorage`、`tso`、`router`、`resourcemanager`、`scheduling` 的依赖顺序迁移；业务侧按 `keyspace`、`gc`、`replication`、`encryption`、`unsaferecovery` 的实际依赖进入 | 每个服务可独立和嵌入运行，watch 恢复、服务发现、主从切换与跨服务协议通过差分；不复制 monolith 状态 |
| 9. 运维、客户端与工具 | `audit`、`maintenance`、`metering`、`gogc/gctuner`、系统资源与版本信息、`swaggerserver`、Dashboard 适配；再按需求迁移 Rust client、`pd-ctl`、recover、backup、simulator 与诊断工具 | 运维 API、指标告警、备份恢复和工具退出码兼容；Go client 在服务端切换期间继续受支持 |
| 10. 生产元数据后端与发布切换 | 新 ADR 选择并实现内嵌共识与持久化；随后进行快照/恢复、混合版本、滚动升级、长稳、性能与 Go 退役 | 无外部 etcd 启动，崩溃一致性和成员变更通过故障矩阵；具备可回滚发布与数据恢复证据后才允许退役 Go |

## 包级准入与完成门槛

- 准入：列出全部生产文件、公开符号、直接仓库依赖、反向依赖、Go 测试、failpoint、指标、持久化 key、协议/API 和并发后台任务；明确 canonical crate 与禁止依赖。
- 计划：一个包若包含多个独立行为，包级计划必须继续拆成聚焦任务；行为变更先记录失败验证，再做最小实现。
- 完成：Rust 测试与 Go/Rust 差分均通过，错误/指标/序列化/持久化兼容，故障与取消路径无泄漏；上层只能依赖已公开且已验证的 Rust API。
- 切换：包完成不等于流量切换。切换由所属主题的集成门统一控制，保留 Go oracle，直到上层调用链和回滚路径均得到验证。

## 风险控制

- 直接翻译 Go import 图会把 proto、etcd、全局日志和大接口带入领域层；包级计划必须先切边界再移植行为。
- `pkg/core`、`server/cluster`、`pkg/schedule/schedulers`、`server/api` 是高扇入/高复杂度区域，禁止作为单个实现任务；必须按稳定公开行为继续拆分。
- 已有 Rust 基础 crate 可能只覆盖纵向 MVP。任何“已有同名 crate”的包都先做 gap analysis，不能把现有实现自动标记为完整迁移。
- Go 版本持续演进时，包级计划固定基准 commit，并在完成前重放基准后的相关 Go 变更，避免以陈旧 oracle 验收。

## 开发策略

- 对会改变行为的代码，使用失败验证-通过验证-重构。
- 在实施步骤前规划失败验证命令和预期失败。
- 将实施步骤限制在失败测试所证明的行为范围内。
- 测试真实行为，不验证模拟对象或实现细节。
- 每个包只迁移已被调用方需要的真实能力；不添加推测性的扩展点，也不以桩、忽略测试或零测试作为完成证据。

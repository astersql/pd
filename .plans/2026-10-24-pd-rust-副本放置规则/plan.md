# 副本放置规则计划

目标：以小任务迁移或补齐 schedule/placement 的真实公共行为。

范围：规则值与标签约束;规则排序与覆盖列表;配置补丁与引擎限制;规则管理器加载与单条变更;批量规则与组事务;单规则角色适配;多规则区域适配搜索;隔离评分与替换判断;适配缓存失效;管理查询与适配集成。

范围外：其它包的业务实现、完整 scheduler 与 Go 修改。

假设：以 `db1b56605b43b973afbd1eef8d03a7e7ba25e0bf` 的 Go 包和相邻测试为基准；扩展已存在 canonical crate，尚未存在的 crate 按首任务创建。

## 设计决策

- 比较整包翻译与逐行为增量，选择逐行为验收；一个稳定 Go 包对应一个 canonical crate。
- config 复用现有值模型；持久化编排在 server/config，使用现有 endpoint。
- 比较并行与串行，因共享接线和格式化选择串行。

## 架构说明

- types/keyrange/rangelist/plan/config 支撑 labeler/placement/affinity；禁止反向依赖 statistics/operator/schedulers。
- 领域类型隔离协议；内存行为与 Etcd 恢复共同验收。

## 固定边界

- 来源：`/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-22-pd-rust-渐进迁移主题/4-迁移调度基础契约与规则.md`，状态为“未开始”，本次仅拆分，不修改来源任务及其只读 plan.md。基准提交：`db1b56605b43b973afbd1eef8d03a7e7ba25e0bf`；执行前比较此提交后目标 Go 包的变更并记录。
- 前置：主题 3 的相关 core/storelimit、keyutil、kerneltype、configutil、storage/kv、storage/endpoint 公共接口已完成所需验证；有 crate 不代表能力完整。缺口先按任务阻塞规则处理，禁止新增平行实现。
- Go 只读约束优先于执行技能。开始和结束记录 `.go` 与 Go 模块文件的 Git 状态、内容哈希，包含未跟踪文件；保留用户已有修改。禁止 failpoint 启停、make 测试、Go 格式化和模块整理。
- Go 源码和已有测试提供期望依据；真正运行的 Go 输出才可称为运行差分。执行已有二进制或测试前审查完整调用链、源码写入和 failpoint 需求；不能只读获得必要 oracle 时记录缺口并阻塞对应验收，不新增 Go oracle 工具，不以源码推导冒充运行证据。
- 领域模型不依赖 proto/transport；转换在适配层，存储使用现有 MetaStore/endpoint。禁止 Go FFI、全局状态、私有 Tokio runtime；后台任务必须取消并 join。
- 默认 features，不增加推测性开关。生产 Rust 以 `// Copyright 2026 AsterSQL.` 开头；测试为独立 `*_test.rs` 并显式登记。新 crate 只在对应包注册 canonical workspace member/dependency，集中依赖并更新 Cargo.lock，扩展已有 crate 不重复创建。
- 所有任务都串行；编号是线性顺序，批次号为全局号。共享 manifest、lockfile、lib.rs、全 workspace 格式化、提交和差分环境都可能冲突，Cargo 分槽不解除这些互斥。

## 开发策略

- 固定 Go oracle 场景与失败证据，先失败验证，再最小实现和通过验证，最后重构。
- 缺少测试名称、零测试、ignore 或只有编译成功均不算完成。配置已有实现通过的场景只记录覆盖；实际缺口必须有新增失败行为测试。
- 内存使用真实 MetaStore，持久化用真实 EtcdMetaStore；只允许在明确 I/O 故障边界注入错误。不得模拟 fit/匹配结果。
- 实际变更使用 `$git-commit` 提交且排除其它任务和用户变更；只有任务文件自身的状态变更或删除不提交。完成证据在删除任务文件前写入对应证据文档与最终回复，plan.md 始终只读。

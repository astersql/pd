# 控制面与MCS拆分计划

目标：将主题8拆为包级小任务，完成领域服务与MCS的真实行为、故障恢复和双部署模式验收。

范围：replication、encryption、unsaferecovery、keyspace、GC；MCS公共工具、注册、发现和生命周期；MetaStorage、TSO、Router、ResourceManager、Scheduling，以及必要的tsoutil与领域接线。

范围外：主题7通用API缺口、主题9通用运维和客户端移植、主题10原生存储与生产切流；不修改任何Go源码或模块文件。

假设：已存在Rust workspace、MetaStore、runtime、server、tso、调度领域crate；encryption与rm-server已有配置，不代表服务完成。主题7尚有未完成任务，实施需核对准入。

## 设计决策

- 比较整服务搬运、按函数翻译和按可测行为切分，采用第三种，避免状态机副本。
- 独立子系统分别建立子计划；公共接线顺序执行，未就绪依赖不得假定完成。

## 架构说明

领域层复用MetaStore及既有调度、TSO状态机，proto只进适配层。库接受宿主运行时与取消令牌；外部etcd仍为过渡后端。

## 开发策略

- 遵循失败验证、通过验证、再重构；源码推导与真实运行差分分开记录。
- 包级plan.md只读，进度只记对应编号任务；全局线性顺序及批次在prompt.md，各子目录保留对应提示。
- 共享Cargo.lock、模块注册、全局格式化、提交和真实集群资源，保守采用一任务一批次串行。槽位只隔离构建缓存，不授权并行写源码。
- 当前工作区有他人正在修改runtime/server和只读差分脚本，执行者先比对快照、避免覆盖，并等待相关任务完成后复用最新实现。
- 新crate是拟建清单：首次真实行为任务创建manifest、lib及workspace注册，不创建无行为空壳。已有crate扩展原位置；下层库不得依赖runtime。
- 每个包首次执行先固定来源符号、调用方、测试和直接依赖清单，发现遗漏映射先报告并按技能征询是否更新来源，不擅改来源任务或本指南。
- 根构建脚本现仅使用已缓存协议ZIP并解压proto；执行前重新核对只读性，缺少缓存记录阻塞，禁止下载解压Go源码绕过约束。
- 实现阶段可按仓库现行惯例补生产Rust源旁说明；证据存入docs/rust/control-plane-mcs，不以文档代替真实行为。

## 来源与架构依据

- [主题8来源](/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-22-pd-rust-渐进迁移主题/8-迁移控制面服务与MCS.md)（状态未开始，本次不修改）
- [总体主题边界](/Users/Shared/work/dir/data/codes/pd/.plans/2026-10-22-pd-rust-渐进迁移主题/plan.md)
- [ADR-001](/Users/Shared/work/dir/data/codes/pd/docs/rust/architecture.md)
- [移植指南](/Users/Shared/work/dir/data/codes/pd/docs/rust/porting-guide.md)
- [依赖基线](/Users/Shared/work/dir/data/codes/pd/docs/rust/dependencies.md)

## Go 只读硬约束

- Go 仅供只读参考。禁止新增、修改、删除、重命名任何 `.go` 文件（含生产代码、测试、oracle、生成文件及临时目录中的 Go 工具）；`go.mod`、`go.sum`、`go.work`、`go.work.sum` 同样只读。所有新增实现与测试使用 Rust，必要的非 Go 验证脚本和证据文档须在任务范围内。
- 禁止通过生成器、格式化、依赖整理、failpoint 注入或构建脚本间接改写 Go；不得运行会改写 Go 的 make 测试入口、failpoint 启停命令、`go generate`、`go mod tidy` 或 Go 格式化写入命令。不得以“只增加测试/工具”“临时修改后还原”或扩大任务范围绕过约束。
- Go oracle 仅复用已有只读源码、测试、fixtures、已保存输出或已有二进制；运行已有 Go 工具/测试前，必须确认整个调用链不写上述文件且不需要 failpoint 注入。需要 failpoint 的测试不得直接裸跑来替代验证；无法只读取得必要运行证据时，记录具体缺口并按任务阻塞规则处理，不降低验收标准，不将源码推导冒充运行差分。
- 开始与结束时核对上述文件的 Git 状态及内容，保留用户已有变更；本任务不得产生任何 Go 文件或 Go 模块文件差异。本约束优先于计划、执行技能或验证要求中任何相冲突的写入指令。

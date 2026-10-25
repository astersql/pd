# 任务 008：容量、标签、引擎与隔离过滤

状态：已完成。批次 8；依赖批次 7 提交 `c6de386d2`，其真实 Filter、异步
状态过滤和运行证据已核验。任务使用其同步 Filter 和真实 core/config/plan API，
复用现有 placement 领域契约。整体及包级 plan.md、来源主题任务均只读。

## 实现

[store_constraints.rs](../../../pkg/schedule/filter/store_constraints.rs)公开
StorageThresholdFilter、LabelConstraintFilter、EngineFilter、SpecialUseFilter、
IsolationFilter、HotRegionEvictedTargetFilter，以及 special_engines 和
not_special_engines。保留 Go filter 身份 1/3/6/7/8/9、source/target 差异、
状态码及详细原因。无新 feature、状态副本、I/O、私有 runtime 或 mock 预算。
[领域说明](../../../pkg/schedule/filter/store_constraints.rs.md)记录容量与静态
约束的职责、错误和构造器边界。core 仅公开已有 equal_fold 函数供隔离过滤复用，
没有复制或修改折叠算法。Cargo.lock 仅增加现有 placement 包的本地依赖边，
无远程版本漂移。placement既有rule manager经storage-endpoint传递依赖proto
（独立keyspace持久化适配）；本过滤器的公开API和源码只消费领域类型，无直接
proto或server/runtime依赖。本任务不重构既有持久化适配边界。

## Go oracle 与覆盖

[fixture](fixtures/task_008.json)、[provenance](logs/task_008/provenance.json)与
[来源哈希](logs/task_008/sources.json)固定 Go commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`。复用前序 filter.test、checker.test
和 dlv 二进制，核验 SHA256；未创建、修改、生成或重新编译 Go 源码。
[freeze.py](logs/task_008/freeze.py)与对应 commands/json 保存原始输入、返回和
确切命令。实际冻结138组、276次 source/target 返回：

| 路径 | 案例 | 主要边界 |
| --- | ---: | --- |
| constraints | 50 | 容量10、引擎10、specialUse30；0容量、ratio阈值两侧、8GiB±1、99/100 region、空/未知引擎、allowUses和低容量source豁免 |
| hot | 8 | slow/stopping/trend三标记全部组合 |
| labels | 37 | in/notIn/exists/notExists/invalid、空值、大小写键、engine/$/exclusive gate、空约束与空labels |
| isolation | 20 | zone、空region、缺失值、ASCII和Unicode简单折叠，含K/k、Σ/ς、ß/ss、İ/i |
| isolation_rack / isolation_host | 16 | 真实层级前缀、同rack异host、同zone异rack、大小写及空值 |
| constructors | 7 | zone/rack/host、未知/空/大写层级回退、空location与空region |

API探针仅改变领域输入并调用真实生产函数，不改算法、返回或断言；改变输入后
退出，不冒称完整原Go测试通过。checker路径的单元素 NotSpecialEngines slice
被作为构造器显式输入缓冲使用，in/notIn和allowUses分别写入，未替换匹配算法。
隔离路径显式设置label键以排除map迭代顺序；region约束是构造器已有快照，
额外修改的约束值在fixture中作为region输入重建。构造器调用显式展开嵌入接收者，
避免dlv对Go promoted method的错误接收者解释。

[safe-tests.json](logs/task_008/safe-tests.json)另行完整运行10项现有Go测试，
退出0。TestMain/goleak/WaitForEtcdConnections及所选setup/assertion路径已检查，
这些断言不需要failpoint注入。计划列出的DistinctScore、RuleFit、
RuleFitWithPlacementRule、SendState仅为相邻行为回归；不把其测试名当成本任务
六种过滤器的直接证明。直接证据为冻结返回以及LabelConstraints、
HotRegionEvictedTarget、Isolation、IsolationCaseInsensitive、SpecialUse、
SpecialEngine现有完整断言。当前classic/default输入不引入Go build tag对应的
新Cargo feature。

逐字段归一化仅移除调试指针、栈帧和日志时间；完整保留status code和
DetailedReason。没有时间或随机业务字段需要归一化。

## TDD 与故障

[attempts.json](logs/task_008/attempts.json)记录缺失API的编译失败101（零行为
测试，不当作完成证据），及首个运行因Rust默认配置未显式设置Go的0.8容量阈值
而失败101。修正等价测试输入后3项通过。
[mutation.py](logs/task_008/mutation.py)在同一注册测试上逐项引入并还原：
容量放行、标签放行、引擎反转、取消specialUse低容量source豁免、隔离比较
大小写敏感、忽略stopping驱逐。六次均触发task_008_behavior_cases断言失败，
退出101；[原始结果](logs/task_008/mutation.json)和各mutation日志保留。

[fault.py](logs/task_008/fault.py)使用真实region stores和空location labels
调用Go隔离构造器，观察到runtime.boundsError，x=0/y=0/BoundsIndex；
[fault.json](logs/task_008/fault.json)保存原始故障。Rust独立should_panic测试
验证相同索引故障，不比较语言特有panic文字。额外链测试使用真实store、
两种过滤顺序、首失败status、重复Arc输入和source/target/unavailable选择，
并验证未知层级和allowUses边界。本任务无持久化访问，不构造无关I/O故障。

## 验证

[validation.json](logs/task_008/validation.json)与validation-0.out至
validation-6.out保存命令输出、退出码和计数。三个显示日志仅去除EOF空白行以满足
Git whitespace检查；[validation-raw.json](logs/task_008/validation-raw.json)保留
其逐字完整stdout/stderr文本，业务输出和测试计数没有归一化。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-filter --locked --lib task_008 -- --nocapture` | 0 | 3通过/0失败/0忽略；138组/276返回 |
| `cargo test -p astersql-pd-schedule-filter --locked` | 0 | 14通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace编译 |
| `cargo test --workspace --locked` | 0 | 1340通过/0失败/1既有忽略（tso_processes） |

[slot-run.py](logs/task_008/slot-run.py)按原子mkdir动态领取1–10槽位，记录
plan/task/session/PID，向所有Cargo和子进程传递绝对CARGO_TARGET_DIR。
本任务全部构建阶段实际领取槽位1，使用
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；正常/失败/中断都等待
子进程退出后释放自有锁，保留缓存。[最终阶段](logs/task_008/validation.out)
及[变异阶段](logs/task_008/mutation-stage.out)包含领取与释放结果。

[verify.py](logs/task_008/verify.py)重新解析全部原始Go返回核对fixture、
核验来源/二进制SHA256、全部Go和模块文件（含untracked/ignored）起止内容、
17份只读plan.md、无server/runtime依赖及既有传递proto链、远程依赖版本、
变异和有效测试计数与文档链接。初次审计将所有传递proto依赖一并禁止，因上述
既有placement链触发；已改为准确检查领域API不使用协议类型并显式记录传递链，
没有通过删除依赖或复制领域实现规避问题。
[Go审计](logs/task_008/go-verification.json)和
[证据审计](logs/task_008/evidence-verification.json)记录最终结果。
`git diff --check`通过。只提交本任务代码、接线、测试和证据；完成的编号任务
文件按要求删除，不改两级plan.md和来源主题任务。

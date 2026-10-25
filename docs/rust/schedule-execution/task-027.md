# 任务027：普通副本步骤规划

状态：已完成。依赖26为提交 `c26d047db`，真实 Builder 校验与 OpStep/Operator
API 已核实。按用户后续授权保持现有 Go 源码规划契约，不新增产品安全约束。

生产实现：[builder_simple.rs](../../../pkg/schedule/operator/builder_simple.rs)，
[领域说明](../../../pkg/schedule/operator/builder_simple.rs.md)。API 为
`Builder::build_simple(kind, SimpleBuildOptions, allocate)`；显式非 joint 路径复用
canonical Builder，生成真实 Operator/OpStep。原 build 保持前序 NoSteps/PlanningRequired
契约，待后续 joint 路由行为接线。只增加所需 Builder 冻结 metadata、typed error 和
模块注册，未新增依赖。宿主闭包分配新零 ID，Allocation 错误传播并保持首错。

[固定 fixture](fixtures/task_027.json) 有30项案例：Add/Promote/Remove、leader先迁出、
多个替换、light peer、无变化、缺少合法 leader、ID保留/分配/分配错误、距离优先、
旧 peer 优先与混合 promote/demote/add。source_commit 为
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`，builder.go/builder_test.go 的 SHA256、
原始输入和冻结步骤输出均在 fixture。依据 prepareBuild、peerPlan、各 plan/exec/prefer
及 TestBuild 非 joint、TestRecord light-peer 源码断言推导；TestMain/TestOperatorBuilderTestSuite
仅为 Go 入口，未冒充执行覆盖。oracle-index 的已有运行案例不覆盖普通 builder。

依最新授权采用 `source_assertion` / `go_execution=false`，未运行 Go builder、未宣称
运行差分。预期不从待测 Rust 生成，没有时间/随机字段归一化，没有忽略错误或步骤。
没有存储 I/O，因此不添加 MetaStore mock；allocator 错误在真实调用边界注入。

两个有效 Rust 测试（0忽略）：task_027_behavior_cases 对30项固定案例比较每个步骤、
peer ID、origin SendStore、light/down-store、kind、description、region及typed error；
同时检查 origin 所有权与 sticky build error。task_027_safe_intermediate_configurations
枚举32种无降级的 Add/Remove/Promote目标，按真实region状态逐步确认leader不被删除、
转移目标为voter、每个真实OpStep能完成，以及最终目标相等。正常降级步骤由固定案例覆盖。

源码边界已单独保留：origin为store1 voter、store2 voter、store3 learner；target为
store1 learner、store2 voter，输入符合SetPeers前置条件。planReplace优先配对learner
add1/remove3；old-peer评分使leaderBeforeRemove=2，实际冻结顺序为 AddLearner(1)、
TransferLeader(1→2)、RemovePeer(3)、RemovePeer(1)。首步在原region上
CheckInProgress=AlreadyVoter、GetCmd=None、IsFinish=false；真实 Operator.check
重复心跳保持current_step=0。测试验证这一实际拒绝边界，不模拟成功执行或断言不存在的
普遍目标可达性。显式目标leader的正常混合降级也有独立固定案例，未放宽正常契约。

先前65组合的“所有目标都可执行”枚举断言超出Go Builder规划契约，已按用户授权改为
上述范围明确的可达性测试及独立边界测试，两个均显式注册、没有忽略测试。
[safety-investigation.out](logs/task_027/safety-investigation.out) 与
[pre-resume-validation.json](logs/task_027/pre-resume-validation.json) 保留原始失败，
不作为当前验证结果。此前阻塞已解除，无需更改Go或只读计划。

初始缺失API的编译失败保存于red-missing.out。四种真实错误变异（跳过leader迁移、
跳过晋升、忽略light标记、错误snapshot源）均被同一fixture断言识别，退出101；
[red.json](logs/task_027/red.json) 保存确切命令/退出码和恢复生产源码哈希。每次变异
在finally中恢复生产字节，没有以编译失败代替行为失败。
补充 Unicode 标签距离案例使用 Go strings.EqualFold 的 Σ/ς 折叠，ASCII比较会触发
真实步骤次序断言失败（red-unicode.out，退出101）；实现复用 core 的 equal_fold，
未增加新依赖。变异运行前同样执行 cargo fmt --all。

最终完整命令、退出码和有效数量见 [validation.json](logs/task_027/validation.json)。
workspace既有etcd I/O复用槽位中已保存二进制；reuse-go 拦截器拒绝其它Go调用，
fixture-reuse-provenance.json / go-reuse.jsonl 记录复用来源，和operator oracle无关。

所有阶段动态原子领取实际槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；slot-run.py
记录任务/会话/PID，等待所有子进程结束后清理自有锁、保留缓存。
只读文件核对、fixture哈希与diff检查见verification.json。已有其它任务删除不暂存，
不修改任何只读plan.md、来源主题、Go或模块文件。


| 最终验证命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_027 -- --nocapture` | 0 | 2通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 44通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1407通过、0失败、1忽略 |


最终两个聚焦测试通过：30项固定案例和32种目标状态组合，失败0；operator包44通过，
workspace1407通过、0失败、1既有忽略。所有必需命令退出0。源码边界如实保留，未新增
与Go不同的安全约束；源码推导和Rust行为证据未冒充Go运行差分。

最终只读核对1119项（其中Go/模块文件1101项），内容和受保护
Git状态均一致；git diff --check退出0。fixture SHA256为
`33525f2278733a4494ebc45b3fc8d278ca4196bc506d041a62e249ae8bf01740`。
最终确认所有验证子进程结束，自有槽位锁已释放，缓存保留。已审查本任务差异、局部
注册、依赖未变及证据；只提交本任务实际产物，原有其它任务删除不纳入。编号任务
标记已完成后按授权删除，稳定证据保留供批次28核验，不修改任何只读计划。

# 任务 010：区域健康与区域过滤

状态：已完成。依赖批次 9 的提交 `fc1aa13f2`、129 组/258 次运行 oracle、
18 项包测试和 workspace 证据均已核实。当前批次仅执行任务 10，没有并行任务。
两级及其它 `plan.md`、原始主题均保持只读。

[生产实现](../../../pkg/schedule/filter/region_health.rs)公开健康与复制判断、
RegionFilter、pending/down/replicated/empty/witness/snapshot/affinity 过滤器，
以及 select_regions/select_one_region；[领域说明](../../../pkg/schedule/filter/region_health.rs.md)
记录所有权、时钟、异步 limiter 和诊断边界。真实 core、RuleManager、StoreLimiter、
affinity Manager/RegionLabeler 复用已有 canonical 实现；无新 feature、runtime、
协议领域类型或存储写路径。Cargo.lock 只增加三个既有 workspace 路径依赖，
无远程依赖漂移。

## 冻结 Go 运行证据

来源 commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[fixture](fixtures/task_010.json)、[provenance](logs/task_010/provenance.json)
记录原始领域输入、二进制与来源源码 SHA256、mode 和逐字段归一化。
复用 `/tmp/pd-sched005/filter.test` 和 `dlv`；未生成、编译或编辑任何 Go。
已只读检查 TestMain、WaitForEtcdConnections、所选 setup/断言路径；这些测试
不依赖 failpoint 注入。没有运行 make 或 failpoint 启停入口。

[freeze.py](logs/task_010/freeze.py)、[boundaries.py](logs/task_010/boundaries.py)、
[overreplicated.py](logs/task_010/overreplicated.py)通过现有二进制中的真实生产
方法冻结 45 组案例、57 个完整状态返回与 38 个布尔返回：

| 场景 | 组数 | 验证 |
| --- | ---: | --- |
| health | 12 | 规则开/关，健康、pending、learner、down、欠副本、learner 加额外副本 |
| replication | 14 | 两种模式的上述 6 组，加 4 个无 learner 健康副本；严格等量/规则 fit |
| empty | 8 | size=0/1/2/30，total=0/100；有 key 的 size=1 仍拒绝 |
| witness | 5 | 目标 store 0/1/2/3/99，witness 仅在 store 2 |
| snapshot | 4 | Medium 可用/耗尽、busy、缺少 leader |
| affinity | 2 | 同一实际 group，成员/非成员 key range |

每条 route 的 `.commands` 与 `.json` 保存确切 Docker/Delve 参数、逐字
stdout/stderr、exit code。所有正常 route 退出 0，未有 debugger 命令错误。
归一化只排除调试地址、frame 和日志时间；StatusCode、DetailedReason 和布尔
结果原样保留。snapshot 只依赖 leader store，empty 不依赖 key 数。

[fit_cache.json](logs/task_010/fit_cache.json)另行观察真实 RegionReplicatedFilter：
初始 fit=nil；三个真实 store/peer 成功返回 0 并缓存 fit；减为两个 peer 后返回
1004，fit 地址仍相同。只修改领域输入并调用生产方法，不冒称修改输入后的整个
Go 测试通过。Rust 使用 Arc::ptr_eq 验证同一缓存语义。

[safe-tests.json](logs/task_010/safe-tests.json)独立完整运行 8 项既有 Go 测试，
退出 0：TestIsRegionHealthy、TestRegionPendingFilter、TestRegionDownFilter、
TestRegionReplicatedFilter、TestRegionEmptyFilter、TestRegionWitnessFilter、
TestSendStateFilter、TestAffinitySelect。实际断言覆盖与上表的新增边界分别记录，
不以测试名代替输出证据。

## TDD 与故障

[独立注册测试](../../../pkg/schedule/filter/task_010_test.rs)首先取得
[缺失 API 编译失败](logs/task_010/red-compile.out)，退出 101；此编译失败不算
行为断言证据。first/second/third-pass 日志保留测试装配修正过程：store slice、
Status 字段和 collector step 校正；ProviderSnapshot 零值须显式设置 Go 测试的
max_replicas=3 和规则开关；新增 snapshot 输入须装配 busy/no leader。

[mutation.py](logs/task_010/mutation.py)在同一已注册测试上分别引入并恢复
10 项错误：[结果](logs/task_010/mutation.json)全部退出 101，且各有至少一个
真实断言失败：允许 pending 健康、允许 down 健康、接受超量副本、绕过规则拒绝、
失败清除成功 fit、空区域阈值含 100、witness 不按 store、忽略 busy、绕过 affinity、
反转选择顺序。变异审查暴露超量副本缺口后，补充了真实 Go 运行 oracle，重新执行
全部变异通过；最终源码在 finally 恢复并重新格式化/验证。

四项测试完整覆盖 45 组冻结案例；额外性质断言检查 fit 保留、顺序、Arc 身份、
空输入、重复项、collector 首拒绝/step、nil affinity manager、snapshot 捕获
后不随 limiter 改变。选择顺序与这些额外所有权性质依据只读源码设计，不冒称
每一性质都有独立 Go 返回 fixture。down/pending、规则/副本拒绝、snapshot 耗尽/
busy/缺 leader 是本任务故障边界。过滤器无持久化 I/O，不构造无关存储故障；
测试依赖以真实 MemoryStore-backed RuleManager/RegionLabeler 装配。

## 最终验证

[validation.py](logs/task_010/validation.py)、[validation.json](logs/task_010/validation.json)
保存确切命令、逐字 stdout/stderr、exit code 与有效测试数。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-filter --locked --lib task_010 -- --nocapture` | 0 | 4 通过，0 失败/忽略；45 组差分案例 |
| `cargo test -p astersql-pd-schedule-filter --locked` | 0 | 22 通过，0 失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace 编译 |
| `cargo test --workspace --locked` | 0 | 1348 通过，0 失败，1 既有忽略 |

[slot-run.py](logs/task_010/slot-run.py)每阶段通过原子 mkdir 从槽位 1–10 动态领取，
记录 plan/task/session/PID，在正常、错误与信号路径等待子进程后释放自己的锁。
所有 Cargo 阶段实际使用槽位 1，绝对 CARGO_TARGET_DIR 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；
[最终阶段](logs/task_010/validation-stage.out)记录领取和退出。
没有残留本任务锁，缓存保留；未移除其它任务锁或缓存。

[verify.py](logs/task_010/verify.py)重新解析全部原始返回、fit 指针及测试结果，
核对 [Go/模块起止哈希](logs/task_010/go-verification.json)（包括 untracked/ignored）、
17 份只读 plan.md、二进制/源码哈希、远程依赖版本、生产依赖方向与 Markdown 链接。
[审计结论](logs/task_010/evidence-verification.json)保存有效计数和 fixture SHA256。
`git diff --check` 通过；仅提交本任务实现/接线/测试/证据，不提交其它任务或用户变更。
完成后删除编号任务文件，不改写两级计划或原主题。

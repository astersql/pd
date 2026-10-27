# 任务135：心跳到操作终态闭环验收

状态：已完成。依赖134的提交 `d51202c40`、[证据](task-134.md)及
`astersql_pd_server::schedule_wire::{to_pdpb,to_scheduling}` 已核实。
本任务仅新增组合测试、显式集成 target、既有 workspace 依赖边和验收证据，
没有新增业务调度实现或更改 Go。根与包级 plan.md 保持只读。

## 来源与证据边界

按用户后续明确授权，使用**只读 Go 源码/测试推导 + Rust 真实行为及故障验证**。
没有获得或宣称新的 Go 运行 oracle、Go/Rust 运行差分、TiKV/Raft 集群运行或
18 类 scheduler 各自的完整进程闭环。准入 oracle-index 中既有输出不足以作为
本组合故事的完整 oracle；仅用于核实先前证据的适用范围。
[provenance](logs/task_135/provenance.json)冻结来源 commit、所有采用的 Go 源码哈希、
准入索引和[固定输入/期望](fixtures/task_135.json)哈希；15份 Go 文件均与
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 相同。
没有时间/随机字段归一化，完整命令重发作逐字段相等比较。

Go 来源为 cluster heartbeat 路径、LearnerChecker/JointStateChecker、
builder.go 的普通与 joint 规划、operator.go/step.go 的完成/confver定义、
Controller.Dispatch/GetOpInfluence/相关 merge 清理，以及四个 scheduler 的具体策略。
固定输入显式设置3副本和 joint-consensus，不把未 Adjust 的配置零值当作 Go 默认配置。
期望为源码推导，未从 Rust 输出生成。初始输入配置与期望错误的修正过程保留在运行日志。

## 实际路径与覆盖

[集成测试](../../../tests/rust-compat/schedule_loop_test.rs)通过
`tests/rust-compat/Cargo.toml` 的 `[[test]] name="schedule_loop"` 注册。
生产组合路径为：

`StatePipeline::region` → canonical core/统计/placement/StorageEndpoint →
真实 checker、scheduler 或操作工厂 → `ControllerAdmission::add_operators` →
`ControllerDispatch::dispatch` → `StreamRouter` → 两种 `schedule_wire` 转换与 prost 解码 →
外部 TiKV 命令应用边界 → `StatePipeline::region` → 操作终态及影响量查询。

测试宿主只装配现有组件；没有 mock cluster trait、复制生产决策到 compat crate、
新 feature 或私建 runtime。外部 TiKV 边界只消费解码后的 wire command，
不读取 Operator 步骤来生成完成心跳；单成员 V2 自动稳定、split/merge 的外部
状态变化均为显式受控输入模型，未替代 TiKV 共识或持久化实现。
MetaStore 使用真实 MemoryStore；其事务 I/O 故障才由 FaultStore 注入。

`task_135_behavior_cases` 共21个有效模式案例：learner晋升、joint恢复、peer迁移、
witness双向转换、split、merge，以及 evict-leader/grant-leader/shuffle-leader/
shuffle-region 四个 scheduler 的实际决策。20个案例确认完成通知，1个源码定义
的失败路径确认取消；merge每例还确认相关源操作取消。34条首次命令加34条完整相等
的重发，验证重复心跳不推进步骤；旧 epoch 心跳被拒绝且保持当前 Arc 身份。
逐例核对命令类别/顺序、目标成员和角色、leader、confver/version、完整范围、
区域数量、大小/keys、真实 placement fit、持久化内容和清理结果；终态活跃影响量为零。
learner/joint恢复完成后重新检查不再生成操作。

18类 scheduler 的独立策略验收与上述组合验证共同提供覆盖。
[覆盖矩阵](logs/task_135/scheduler-coverage.json)逐一映射具体行为测试、固定fixture哈希；
[校验脚本](logs/task_135/coverage.py)对照实际日志确认通过，并核对18类完整集合。
本次重跑该包192项测试，包含所有18类实际策略、步骤/目标及其故障测试，
没有以工厂实例化或注册数量替代策略运行证据。

两个额外真实测试：

- `task_135_route_failure_retry_and_stale_confver`：断开的路由保持 STARTED，重新绑定后
  完成；未授权 confver 增长会取消操作且不发送命令，两条路径均清理活跃影响量。
- `task_135_metadata_failure_is_not_rolled_back_or_retried_by_duplicate`：写失败保留 root
  并返回 SaveRegion 错误；重复心跳不重试写入，新的 confver 心跳修复持久化；
  stop/join 后统计快照明确失败。全部 runner 与 HotStat 正常关闭。

### 不能误报为成功的来源边界

Go普通规划在 witness voter 恢复为普通 voter 时执行 remove/add learner，
保留同一 peer ID。完整 confver 回传后，RemovePeer.ConfVerChanged 不再承认移除，
Controller.checkStaleOperator 将操作以 `epoch not match` 取消。
本任务保留此非joint/V1输入及正确失败结果（2条命令，完成0，取消1，
剩余 witness learner），不忽略 confver、不换 ID、不降低断言来伪造收敛。
对应 V2 joint 路径通过 demote/switch/promote 正常完成。
这是源码推导下的来源行为边界，不是新 Go 运行缺陷确认或生产修复声明。

merge按确定的宿主事件顺序先回传存活目标心跳，再处理失踪源的通知队列：
目标 SUCCESS 并由相关操作清理取消源；不要求失踪源本身变成 SUCCESS。
这与 Go removeOperatorWithoutBury/removeRelatedMergeOperator 的定义一致。

## 失败、通过与最终验证

[registration](logs/task_135/registration.json)记录首次缺锁文件依赖边的101错误；
[compile](logs/task_135/compile.json)记录初次API接线编译错误。
`cargo generate-lockfile --offline`仅用于既有 workspace 依赖边；意外漂移的
pulldown-cmark已恢复，最终无远程依赖版本改变。
[green1](logs/task_135/green1.json)至[green5](logs/task_135/green5.json)保留输入与
源码期望校正过程，不将编译失败或错误期望当作缺失生产行为证明。
[strengthened](logs/task_135/strengthened.json)确认加入重复下发及范围/epoch断言后通过。

[变异脚本](logs/task_135/mutation.py)临时移除生产 ControllerDispatch 的完成通知，
[red](logs/task_135/red.json)记录同一集成目标退出101：3项测试中1通过、2项行为断言失败。
finally恢复原文件；生产文件无最终差异。恢复后重新运行全部最终验证，
没有靠缺包、缺注册或零测试满足失败验证要求。

所有Cargo阶段由[槽位脚本](logs/task_135/verify.py)原子mkdir动态领取1–10空闲槽位。
实际每阶段为槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录任务、计划、命名会话与PID；脚本继承该绝对路径到所有子进程，
正常、错误、信号中断先等待/终止并join子进程组，再释放自己的锁，保留缓存。

最终命令、完整合并stdout/stderr压缩日志、退出码、摘要和日志哈希见
[final.json](logs/task_135/final.json)。

| 命令 | 退出码及有效结果 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo test -p astersql-pd-compat-tests --locked --test schedule_loop -- --nocapture` | 0；3通过，21表驱动案例 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib -- --nocapture` | 0；192通过，18类覆盖 |
| `cargo test -p astersql-pd-compat-tests --locked` | 0；23通过，1既有忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0 |
| `cargo test --workspace --locked` | 0；1848通过，0失败，1既有忽略 |

包级及workspace既有TSO进程测试保持原先的忽略状态，无新增忽略。
workspace测试需要的外部etcd I/O fixture复用task030已保存二进制及固定SHA256；
[Go拦截器](logs/task_135/reuse-go/go)仅复制该二进制到本阶段自己领取的槽位，
拒绝所有真实Go命令，[复制记录](logs/task_135/go-reuse.jsonl)保留每次操作。
此前聚焦编译未执行proto生成器；未运行Go测试、注入、格式化或模块整理。

[最终审计](logs/task_135/audit.json)和[审计脚本](logs/task_135/audit.py)核对1101份
Go/模块文件集合、内容与Git状态，根和包级plan.md、最终日志及Markdown链接。
格式化未带入无关Rust变更；已有其它编号任务删除不纳入本提交。

# 任务040：副本位置选择策略

状态：已完成；最终命令、计数和只读审计见下文证据。

实现：[replica_strategy.rs](../../../pkg/schedule/checker/replica_strategy.rs)，
[领域说明](../../../pkg/schedule/checker/replica_strategy.rs.md)，
[测试](../../../pkg/schedule/checker/task_040_test.rs)。
canonical crate 为 `astersql-pd-schedule-checker`，只增加该 crate 的模块注册和
config/filter/placement 依赖；Cargo.lock 仅增加两个路径依赖，无远程版本变化。

## 依赖、来源及证据性质

已核实批次39提交 `269a5dc88` 和 [task-039](task-039.md) 的完整通过记录。
复用真实 core StoreInfo/RegionInfo、filter StoreCandidates/StoreStateFilter/
DistinctScoreFilter/IsolationFilter 和 placement fit_region/RegionFit/RuleFit。
RegionFit 在 fit_isolation.rs 已提供 `rules()`，最终没有修改 placement。
审查 `oracle-index.json` 后确认其运行用例未覆盖该选择策略。

依用户后续授权，Go 运行 oracle/差分不作为准入条件。本任务**没有运行 Go**；
期望均依据只读 Go 源码和已有测试推导，不声称运行差分。
[fixture](fixtures/task_040.json) 明确 `evidence_kind=source_assertion`、
`go_execution=false`，记录实际源码提交 `269a5dc88` 的完整 hash、12份来源 SHA256、
固定 store/region/config 输入和预期输出。额外3组比较边界及6组 witness 输入在
独立 Rust 测试中显式列出。没有时间/随机结果归一化，时间是固定输入。

来源覆盖 replica_strategy.go，调用方 replica_checker/rule_checker，placement
fit/label constraints，filter candidates/store-state，以及 versioninfo 的
SwitchWitness=6.6.0 和 IsCompatible。相邻测试重点包括
TestRemoveExtraReplicaKeepsUnhealthySource、TestFixBetterLocationEngineConstraint、
TestAffinityBestLocationFilteredByTemporarySourceState；仅作源码契约来源。

## 行为及故障覆盖

- Add：先永久约束与最高位置评分，再临时状态与最低 region score；不因 busy/
  snapshots 回退到较差位置。支持 caller/strategy extra filters、排除已有 region
  peer、reserved special-use、isolation、无解、相同位置负载比较和平分顺序。
- Fix：允许位置退化、空集合、singleton 保留比较对象、缺失旧 ID 的 Go 首项丢弃行为。
- Improve：严格改进、相同位置拒绝、空集合、缺失旧 store、无标签无严格改进。
- Remove：严格源状态在排名前应用；temp-state 版本先固定最差位置，优先健康同分源；
  全部临时拒绝仍返回最佳源与 true，空集合无解。removed source 仍允许移除。
- Fast failover：URGENT 和 fast target 条件，snapshots 可绕过、busy 仍拒绝。
- Better location：暂不可用源/目标/双侧等待、无改进；同 engine、同规则角色、匹配
  规则集合的过滤，及多个同角色规则的联合比较；丢失 store 跳过且保留 Arc 身份。
- Witness：6.5拒绝、6.6接受、配置禁用拒绝、6.7/7.0接受、旧主版本拒绝。
  宿主投影版本 major/minor，保持 Go 对6.6任意 patch/prerelease 的兼容判断。

策略返回 Go 的 ID/临时拒绝布尔值，没有外部 I/O 或持久化调用点；没有 MetaStore
故障注入适用点。busy/snapshot/removed 是真实 StoreInfo 状态；没有桩、忽略或
以 mock 调用次数替代行为断言，没有新全局状态或私有 runtime。

## 失败验证及回归识别能力

[red.log](logs/task_040/red.log) 首先记录新依赖需要 lock 更新，exit101；
随后 [red-api.log](logs/task_040/red-api.log) 记录缺失模块/API，exit101。
离线 generate-lockfile 后审查并恢复了 pulldown-cmark 意外更新，最终 lock
只增加本任务路径依赖。初次实现误加重复 rules accessor，green.log exit101，
删除该重复访问器后直接使用既有 API。

[green-2.log](logs/task_040/green-2.log)、[green-3.log](logs/task_040/green-3.log)、
[green-4.log](logs/task_040/green-4.log) 保留输入/期望问题暴露过程。
[input-corrections.json](logs/task_040/input-corrections.json) 记录依据：配置零值
不能替代生产 v2/容量比例；空 engine 标签仍属排他标签，应表示为缺失；caller
的目标 engine filter 应显式提供；removed source 不受目标的永久状态限制。
这些修正均回查 Go 契约，未将 Rust 输出反推为 oracle。

[mutation.py](logs/task_040/mutation.py) 在真实实现上逐个引入四种错误，同一组
测试均报断言失败 exit101：提前严格过滤导致次优目标、忽略临时源、witness错误
版本门槛、忽略比较集合的 engine 边界。
[mutations.json](logs/task_040/mutations.json) 及对应 mutation-*.log 保存结果。
首次 engine mutation 被规则角色条件遮蔽，记录在 mutations-first.json 和
mutation-engine-first.log；强化为同角色 TiFlash 规则后成功识别 engine 错误。
脚本 finally 恢复源文件并格式化，之后执行完整验证。

## 验证与共享槽位

[run.py](logs/task_040/run.py) 每阶段从1–10依次 atomic mkdir 动态领取，写入
plan/task/session/PID；子进程继承绝对目录，正常/失败/中断等待退出后释放自己的
锁并保留缓存。所有实际阶段使用槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

[validate.py](logs/task_040/validate.py)、[commands.json](logs/task_040/commands.json)
及 validation-0.log 至 validation-6.log 保存确切命令、退出码和 stdout/stderr。
沿用前置验证的 `RUST_TEST_THREADS=2`。唯一既有忽略为 `tso_processes`，需独立 Go/Rust process binaries，本任务未新增忽略。
最终统计见
[summary.json](logs/task_040/summary.json)。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_040 -- --nocapture` | 0 | 3通过/0失败/0忽略；40表格场景+3比较边界+6门槛 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 5通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1463通过/0失败/1既有忽略 |

[audit.json](logs/task_040/audit.json) 保存 fixture hash、全体 Go/模块文件逐一
前后内容比较、只读 plan 检查和差异审查结果。日志仅清理行末空白/冗余末尾空行，
原始与清理后哈希见 log-normalization.json，未更改测试输出内容。
Go/模块文件无本任务变更；两个
plan.md 未改；已有其它编号任务删除保持原状并排除出提交。未生成 Go 或 failpoint
产物。最终 git diff --check/暂存检查通过，槽位锁释放、缓存保留。

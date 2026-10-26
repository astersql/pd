# 任务074：见证均衡配置更新恢复

状态：已完成。用户已明确授权整个计划“突破啊，允许推导啊”，验收采用
**只读Go源码/测试推导 + Rust真实行为和故障验证**；没有Go运行输出或运行差分。
任务73提交 `985b581aa` 的 canonical 配置与调度API、
包级及全仓证据已审阅，本次包级再次回归其行为。

生产 [balance_witness_config.rs](../../../pkg/schedule/schedulers/balance_witness_config.rs)
提供 `BalanceWitnessScheduler::update_witness_config/encode_witness_config/reload_witness_config`，
[边界说明](../../../pkg/schedule/schedulers/balance_witness_config.rs.md)。
采用任务明确的“存储失败保持原配置”：先保存再发布；编码相同返回false且不写。
保存/读取失败传播Storage，恢复解码失败传播Decode；所有失败保留batch及ranges。
复用真实StorageEndpoint、ClusterPaths、MemoryStore及已有validated配置类型，
没有新增后端/依赖/feature/运行时，也未改Go、模块或两份只读plan.md。

## 来源、差异与证据方式

[provenance](logs/task_074/provenance.json) 保存来源基准commit、当前checkout、前置
commit及源码/fixture SHA256。[fixture](fixtures/task_074.json) 的期望由只读来源及
任务契约冻结，没有调用待测Rust实现生成期望。没有Go运行差分，没有归一化业务错误。
Go故障行为与任务要求有意不同：`balance_witness.go:81-85` 保存失败仍保留新值、
返回成功；Rust按用户继续指令确认的任务契约保留旧值并返回类型化错误。
Go ReloadConfig:209-219直接赋值；Rust恢复要求通过batch/key JSON校验。
这些差异不能被描述为已与Go运行一致。

已尝试/核查的只读路线：

- `oracle-index.json` 没有本任务配置持久化故障输出，前序scheduler的结果不能替代。
- `balance_witness_test.go` 只有调度影响量和transfer两条业务测试，没有配置故障。
- 补查发现 `tests/server/api/scheduler_test.go:502-548` 确有batch更新/同值/空请求/
  未知字段测试（修正初次仅搜相邻测试的覆盖结论）；没有本任务save-failure/reload
  故障覆盖。其SetupSuite:65-66要求failpoint；禁止裸跑，也禁止注入改Go。
- task001 manifest登记的原生schedulers.test文件现已不存在；仓库target中的已有
  cluster/protocol oracle没有见证配置故障入口，不运行其它任务探针充数。
- 已运行 `bin/pd-server --version` 与 `go version -m bin/pd-server`，均退出0，见
  [调查](logs/task_074/oracle-investigation.json) 与对应原始日志。二进制SHA256已记录，
  x86_64 Mach-O可运行，但Git Commit Hash为None，buildinfo为command-line-arguments，
  没有可验证的当前Go源码绑定。版本查询不是行为测试，未作为oracle。

本次未新建Go harness、未运行需要failpoint的测试，未把版本查询当行为测试。
上述调查保留为历史证据：原Go运行oracle准入曾阻塞本任务；用户后续明确授权
整个计划采用源码/测试推导加Rust真实验证，该项限制已解除，不再构成阻塞。
Go及模块文件仍保持只读；Go输出SHA256保留null，未伪造运行差分。

## Rust行为与故障证据

`task_074_behavior_cases`：6/6固定案例，batch1/10全域、包含/排除范围、多范围、
开放结束key。保存后逐项检查batch、ranges解码及持久化编码；新scheduler恢复后
重复真实schedule决策，各案例验证更新与恢复后的操作数量（12次决策）、region1、
REGION|WITNESS kind和真实命令从store4取消witness、store1成为witness。
core区域索引、StoreInfo、RuleManager、operator factory均为真实组件。
时钟固定1000秒，随机索引0，shuffle恒等；空ranges沿用前置类型全域规范化。

`task_074_faults_and_validation`：非法构造0/11/usize::MAX拒绝；在MetaStore I/O
边界注入Unavailable，验证save失败保留完整活动配置及原持久化值；read失败保持
配置；同值更新在存储不可用时无需I/O；恢复后重试成功；9个非法持久化输入（空、
截断、null、batch0/11/负数/小数/字符串、非法hex）返回Decode并保持原值；
不同ClusterPaths缺失配置保持原值；二进制key和batch1保存/恢复完整。
I/O边界其余操作委托真实MemoryStore，没有伪造调度或存储业务结果。

[red-api](logs/task_074/red-api.log) 缺失API编译失败，退出101。
首次行为装配发现前置空ranges规范化以及REGION附加kind、普通共识命令与联合命令
形态区别，按既有真实契约修正测试，保留first/focused-first/focused-second日志，
这些不被充作预期行为失败。
[red-behavior](logs/task_074/red-behavior.log) 故意在save前发布candidate，
同一fault测试在“failed save must retain batch AND ranges”断言失败，1通过/1失败，
退出101；恢复正确实现后2项通过。这证明同一测试可识别回滚失效。

## 验证

确切argv、退出码、有效计数及Cargo环境见 [logs/task_074](logs/task_074/)。

| 命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_074 -- --nocapture` | 0 | 2通过，6/6配置场景和故障测试 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 33通过、0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过，既有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1582通过、0失败、1既有忽略 |

[counts](logs/task_074/counts.json) 从实际test result行汇总，含doc tests；既有忽略为
需要独立进程二进制的tso_processes，无新增忽略。零测试目标不用于行为验收。
所有Cargo阶段由run.py原子mkdir动态领取共享槽位，实际均为 **1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常/失败/中断均等待子进程结束后释放自己的锁，最终锁已释放，缓存保留。
Rust变更后先运行cargo fmt --all，再执行相应验证；最终源码不含行为突变。

[审计](logs/task_074/audit.json) 核对1101个Go/模块文件SHA256及清单，含未跟踪扫描，
Git状态为空；两份plan.md与HEAD无差异。文档链接和diff检查通过。
日志仅按[说明](logs/task_074/log-normalization.json) 清理行尾空白及EOF空行。
实现、独立测试、fixture与证据单独提交；其它任务已有删除/修改不纳入。
2026-10-26收尾核实见[completion-audit](logs/task_074/completion-audit.json)：
实现提交`5f0225c78`的源码/fixture哈希与测试日志仍匹配，工作树无Rust/Cargo变更，
1101个Go/模块文件及两份只读plan.md均不变，因此复用上述已通过验证，没有重跑Cargo。
本次只更新验收说明及provenance，不新增代码或改变期望。按新授权标记完成并删除
自身编号任务文件；不修改任何plan.md，不更改来源主题或其它任务。

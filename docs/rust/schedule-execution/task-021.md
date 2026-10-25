# 任务021：见证副本切换步骤

状态：已完成。依赖批次20已核实提交 `d623ed641`、[task-020](task-020.md)
及真实 Rust API；本次再次回归前序全部操作器测试。已阅读 oracle-index.json，
其已有输出不覆盖本任务。用户明确解除 Go 运行 oracle/差分准入限制：
本任务 `evidence_kind=source_assertion`、`go_execution=false`，没有运行 Go oracle，
没有把源码推导称为运行差分。Go/模块及两份 plan.md 保持只读。

实现：[step_witness.rs](../../../pkg/schedule/operator/step_witness.rs)，
[领域说明](../../../pkg/schedule/operator/step_witness.rs.md)。公开 BecomeWitness、
BecomeNonWitness、BatchSwitchWitness 的完成、confver、进度检查、影响量、超时、
命令和 Display；错误复用 PeerStepError。step_add 的 validate、slow_timeout 仅
提升至包内可见以复用，不改变行为。无新依赖或 Cargo.lock 修改，无持久化/I/O。
批次 `timeout_nanos` 保留 Go 有符号溢出，Duration API 为位模式视图；需要有符号
超时的消费方应使用前者。

## 来源与场景

[fixture](fixtures/task_021.json) 固定 source commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、5份只读 Go 文件 SHA256、原始输入和
源码推导期望。源码当前字节与该 commit 一一核对。来源为 step.go 三类 witness
步骤及 helpers、region.go、influence.go 与 storelimit 成本表。
相邻 TestSwitchToWitness 覆盖 Learner/Voter/已切换状态，TestNeedStoreHealthCheck
覆盖两方向健康检查区别；没有声称相邻 Go 测试独立覆盖非见证和批次全部边界。
无随机/时间采样，无字段归一化。

[task_021_test.rs](../../../pkg/schedule/operator/task_021_test.rs) 显式注册3项测试：

- `task_021_behavior_cases`：7行×5种角色=35组状态，每组比较两方向 finish/confver；
  覆盖缺失、替换ID、data/witness、pending，两种 joint 角色和未知角色。
  另有ID零的nil getter边界、4阶段批次heartbeat（未切换、部分、全部pending、完成）、
  空批次及3个Display比较。
- `task_021_commands_costs_timeouts`：2种v2参数×4类命令=8次完整Operation比较，
  保留顺序、重复ID和空envelope；6组size/keys比较整个批次StoreInfluence和快照来源，
  另各6组验证来源=目标及SendStore=0；7组×4个超时断言=28个普通超时比较、1个
  批次有符号溢出断言和4个size/witness计数溢出断言。影响使用全region数据，
  region_count不变；包含空区域、20MiB边界、负size与source snapshot成本。
- `task_021_store_faults`：5组elapsed/health×2方向=10项store结果比较，固定30m down
  阈值；覆盖健康、unhealthy、down、可选检查和错误优先级。另验证缺store、缺peer、
  替换ID、批次先转见证再转非见证的首错，以及空批次无错误，共8项断言。

## 红绿与验证

[red-missing.out](logs/task_021/red-missing.out) 为缺API的初次编译失败，退出101。
[first-green.out](logs/task_021/first-green.out) 保留接入时 put_store API 拼写错误的
编译失败；随后根据只读源码纠正测试夹具中的速率/空区域/显式配置期望，未由
待测Rust生成期望。
[red.py](logs/task_021/red.py) 实现后故意移除非见证完成条件的pending检查，
[red-behavior.out](logs/task_021/red-behavior.out) 在固定 `pending-data` 行断言失败，
[red.json](logs/task_021/red.json) 记录退出101；finally恢复源码并格式化。
[green.out](logs/task_021/green.out) 和正式验证均3项通过。最终仅增加fixture说明字段后，
[final-focused.out](logs/task_021/final-focused.out) 再次格式检查和聚焦3项通过。

[validation.json](logs/task_021/validation.json) 及 validation-*.out 保存确切命令、
退出码、stdout/stderr、有效测试数与目标目录：

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_021 -- --nocapture` | 0 | 3通过 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 26通过 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1389通过、0失败、1既有忽略 |
| `git diff --check` | 0 | 通过 |

全部Cargo阶段由 [slot-run.py](logs/task_021/slot-run.py) 原子mkdir动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
退出/失败/信号路径等待子进程结束后只释放自己的锁，保留缓存。
[readonly-validation.py](logs/task_021/readonly-validation.py) 复用前序已存在外部
etcd fixture，[拦截器](logs/task_021/reuse-go/go) 只复制已有二进制并拒绝实际Go命令。
[provenance](logs/task_021/fixture-reuse-provenance.json) 记录其哈希和来源，
[go-reuse.jsonl](logs/task_021/go-reuse.jsonl) 记录请求；这仅用于Rust存储基线的
外部I/O，并非operator oracle。

[verify.py](logs/task_021/verify.py) 核对开始/结束Go、模块和所有plan.md清单与哈希，
[verification.json](logs/task_021/verification.json) 记录fixture哈希、1119个只读文件、
验证结果及槽位释放情况。格式化仅涉及本任务文件；已有任务10–20删除排除在提交外。
任务21完成后删除编号文件，不改只读plan和原始主题。

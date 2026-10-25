# 任务016：操作过期与超时跟踪

状态：已完成。依赖批次15提交 `1fb12ec6f`、[task-015](task-015.md)
及真实 `OpStatusTracker`/`Clock` API 已核实；没有依赖待回归接口。

用户已解除整个计划的Go运行oracle/运行差分准入要求。测试依据只读Go源码与
测试契约，明确标记 `evidence_kind=source_assertion`、`go_execution=false`；
本任务没有Go运行oracle，不声称Go/Rust运行差分。

## 实现与案例

批次15已提前实现本任务所需的 `check_expired`、`check_timeout`。
本任务复用[status.rs](../../../pkg/schedule/operator/status.rs)的canonical实现，
补充公开API契约和[领域说明](../../../pkg/schedule/operator/status.rs.md)，
没有另建重复的 `status_timeout.rs` 或第二份状态容器，没有扩展步骤或控制器。
过期仅针对CREATED并从创建时间计时；超时仅针对STARTED并从启动时间计时。
等于阈值即触发，零时限立即触发。重复检查返回对应终态身份并保留首次到达时间。
同一Mutex保护检查与转换，取消和时限并发竞争只保留一个终态。

[fixture](fixtures/task_016.json)固定来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`和两个Go来源文件SHA256，
[verify.py](logs/task_016/verify.py)验证来源与该commit逐字节一致。
[preparation.py](logs/task_016/preparation.py)记录一次性源码期望冻结和基线采集过程。
只读核实oracle-index.json：其既有运行用例不提供本任务精确时间边界；依照用户
授权使用源码契约，不改写索引或只读plan。

Go `TestCreate`、`TestNonEndTrans`、`TestEndStatusTrans`覆盖创建与状态表，
由批次15行为测试继续回归；`TestCheckExpired`覆盖未到期、到期、重复过期；
`TestCheckStepTimeout`覆盖启动后阈值前后。等号、零时限、不同创建/启动时间、
全部终态与并发竞争由 `status_tracker.go` 的 `< duration`、当前状态检查和锁契约
推导，未将这些新增案例声称为Go测试运行结果。

[task_016_test.rs](../../../pkg/schedule/operator/task_016_test.rs)显式注册3项测试：

- `task_016_behavior_cases`：7状态 × 2检查方法 × 3时间点，加2个零时限，
  共44固定案例；逐项核对布尔、状态、created/started/终态到达时间和重复检查。
- `task_016_delayed_start_uses_started_time`：创建与启动相隔100ns，核对启动瞬间、
  阈值前1ns、等于阈值。此案例可发现误用创建时间。
- `task_016_concurrent_deadline_and_terminal_transition`：CREATED与STARTED各一组，
  每组16个线程同时竞争取消与deadline检查，核对唯一终态、成功返回数量和稳定时间。

仅控制显式单调时钟，无真实sleep，无mock状态机；正常、边界、错误实现和竞争均
使用生产tracker。此内存领域行为不涉及MetaStore或业务I/O。

## 红绿与验证

因为前置任务已有可用行为，先对同一生产实现引入两种错误，再运行新增测试。
[red.py](logs/task_016/red.py)分别把 `< duration` 改为 `<= duration`，把启动
到达时间改为创建时间；[red.json](logs/task_016/red.json)与对应stdout/stderr保存
两次行为断言失败，退出101，排除缺API/缺注册伪红。finally恢复生产实现并格式化。

[validation.py](logs/task_016/validation.py)和
[validation.json](logs/task_016/validation.json)保存实际命令、退出码、stdout/stderr及计数：

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_016 -- --nocapture` | 0 | 3通过，0失败 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 7通过，0失败 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1370通过，0失败，1既有忽略 |

workspace已有存储测试调用Go构建etcd外部夹具。
[readonly-validation.py](logs/task_016/readonly-validation.py)和
[reuse-go/go](logs/task_016/reuse-go/go)只接受原有构建参数并复制批次14已存在的
真实etcd二进制，拒绝实际Go命令；未跳过Rust存储测试或替换业务返回结果。
[provenance](logs/task_016/fixture-reuse-provenance.json)保存源commit、内容hash，
[go-reuse.jsonl](logs/task_016/go-reuse.jsonl)记录复制请求。这不是operator oracle。

[slot-run.py](logs/task_016/slot-run.py)各阶段以原子mkdir动态领取槽位，记录计划、
任务、会话和PID，等待子进程退出后释放自己的锁，缓存保留。
两个阶段实际均领取槽位1，绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[verification.json](logs/task_016/verification.json)记录最终Go/模块全量内容与Git状态
不变、两个只读plan hash不变、Cargo.lock不变、差分检查、来源fixture hash及槽位释放。

仅提交本任务代码契约、测试、接线、说明及证据；排除用户已有任务10–15文件删除。
完成后删除本编号任务文件，不修改整体/包级plan或原始主题，不宣称整个operator已完成。

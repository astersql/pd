# 任务015：操作类别与终态转换

状态：已完成。依赖批次14提交`8536256e5`及
[task-014](task-014.md)的已验证生命周期接口与证据已核实。

## 依据与授权

用户在2026-10-25明确豁免本任务及后续计划任务的“必需既有同源Go运行oracle/
运行差分”准入要求，允许依据只读Go源码与测试契约实现，并以Rust行为测试和回归
验收。Go/模块只读约束继续有效；两个plan与原始主题保持只读。

[fixture](fixtures/task_015.json)标记`evidence_kind=source_assertion`、
`go_execution=false`，依据源码commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`。不是Go运行输出，不声称运行差分；`go_execution`专指operator oracle，
下述真实etcd外部夹具运行另行记录。
[source-fixture.py](logs/task_015/source-fixture.py)从只读`kind.go`的常量/双向映射、
`status.go`的状态表/允许边冻结期望；merge与时间契约按`status_tracker.go`及相邻
测试设计。未从待测Rust实现反推期望，未采纳先前排除的Go运行产物。
fixture保存五个来源文件的SHA256；脚本验证内容与来源commit逐字节一致。

125个表驱动案例：20种kind输入（包括重复、组合、大小写、空值、空格、尾逗号、
非法名称）、15种bit显示、10种状态显示/终态判断、70条状态转换（7条允许边，
覆盖全部终态和u32::MAX）、10种merge ID。JSON日志另覆盖排序、HTML和Unicode转义。

## 实现边界

[status.rs](../../../pkg/schedule/operator/status.rs)新增`OpKind`、`UnknownKind`、
`OpStatus`、`OpStatusTracker`、`Clock`和`AdditionalInfo`，接入唯一canonical crate
`astersql-pd-schedule-operator`。[领域说明](../../../pkg/schedule/operator/status.rs.md)
说明API、时间表示、错误和后续适配边界。

保留kind低bit显示顺序与Go解析不接受`witness`的行为。状态图拒绝终态重开、非法
跳转及未知状态；原子记录created、started和唯一终态时间，拒绝跳转不更新时间。
merge信息区分不存在/空值和非法但非空值；非法、溢出ID读取为0。
无protobuf、全局runtime、独立Operator副本、MetaStore操作或新增远程依赖。

## 红绿测试

[task_015_test.rs](../../../pkg/schedule/operator/task_015_test.rs)显式注册4项测试：

- `task_015_behavior_cases`：源码fixture的115个kind/显示/状态/转换案例，另逐个
  校验700次到达时间查询、拒绝转换保留时间及状态字符串。
- `task_015_expiration_timeout_and_times`：初始时间、阈值前1ns/恰好阈值、零时限、
  每种终态的重复检查、错误状态不被定时转换、默认真实时钟。
- `task_015_additional_merge_information`：10种ID案例、缺失值、JSON转义，以及
  32线程真实容器并发读写。
- `task_015_concurrent_terminal_transition`：并发争取4种终态，只能一个成功，
  到达时间与终态一致且不可重开。

[red-compile.out](logs/task_015/red-compile.out)记录先注册测试后缺失API，退出101。
[mutation.py](logs/task_015/mutation.py)对生产实现逐一引入终态重开、过早到期、
错误接受witness、错误接受+42、沿用旧到达时间5种错误，同一批真实测试全部因行为
断言退出101；[mutation.json](logs/task_015/mutation.json)及对应red日志保留实际结果。
finally恢复源码并格式化，不保留生产测试开关。初次[green.out](logs/task_015/green.out)
退出0，4通过、0失败。

## 最终验证

[validation.py](logs/task_015/validation.py)保存确切命令、stdout/stderr、退出码和计数；
[validation.json](logs/task_015/validation.json)为最终结果：

| 命令 | 退出码 | 有效测试 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_015 -- --nocapture` | 0 | 4通过，0失败，0忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 4通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1367通过，0失败，1忽略 |

仓库已有存储测试会请求`go build`来构建etcd外部I/O夹具。首轮完整Rust回归实际
1367通过、0失败、1既有忽略，见[首轮结果](logs/task_015/initial-validation.json)；
为消除传递构建写Go源码的风险，最终再通过
[readonly-validation.py](logs/task_015/readonly-validation.py)和
[受限命令替代](logs/task_015/reuse-go/go)运行同一套基线。此替代只接受夹具的原始
构建参数并复制已有二进制，拒绝其它Go命令，不执行编译、修改Go源码或注入返回值。
Rust仍使用真实etcd服务读写/故障行为，并未跳过存储测试。

复用的`rust-etcd-fixture-2647`来自本任务首次尝试之前的批次14验证，二进制内嵌
revision为`e064eee5c`，其Go源文件与该commit一致。仅作为外部I/O夹具，不是operator
oracle。[provenance](logs/task_015/fixture-reuse-provenance.json)固定来源与hash，
[go-reuse.jsonl](logs/task_015/go-reuse.jsonl)记录每次复制请求。

[slot-run.py](logs/task_015/slot-run.py)各阶段动态原子mkdir领取共享槽位；所有Cargo
及子进程继承绝对CARGO_TARGET_DIR，结束后释放自己的锁并保留缓存。
实际各阶段均领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
最终基线全部退出0，共23次etcd夹具请求均复用既有二进制。自己的锁已释放，
编译缓存保留。最终阶段记录见[validation-stage.out](logs/task_015/validation-stage.out)。
[verify.py](logs/task_015/verify.py)重新生成来源fixture、核对红绿/变异计数、
Go/模块/plan哈希、锁文件远程依赖不变、etcd夹具来源和槽位释放；
[审计结果](logs/task_015/verification.json)保存实际统计。

提交只包含任务15生产实现、测试、接线与上述证据，排除已有任务10–14文件删除。
完成后删除本编号任务文件；不修改两个plan或来源主题，不自动宣称整个operator包已迁移。

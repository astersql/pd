# 任务025：操作取消替换与记录

状态：已完成。已核实依赖提交 `dc40c3852` 以及 [task-024](task-024.md) 的真实
Operator/OpStep、推进、终态及影响量证据。只读 oracle-index 中已有 controller 并发
删除运行结果不能作为本任务取消原因/历史输出 oracle。依用户后续授权解除 Go 运行
oracle 准入，所有本任务预期为源码推导 `source_assertion`，`go_execution=false`；
未运行 Go operator，未声称跨语言运行差分。整体及包级 plan.md 保持只读。

生产路径：[operator_record.rs](../../../pkg/schedule/operator/operator_record.rs)，
[说明](../../../pkg/schedule/operator/operator_record.rs.md)。局部接线扩展 Operator、
AdditionalInfo，未增加 crate、feature、依赖或存储层。

## 来源与案例

[fixture](fixtures/task_025.json) 固定 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、来源文件哈希，以及15组 scheduler kind
输入/输出和3条 history 预期。其他边界预期在新增测试中冻结，由 Go 源码推导而非
Rust 输出生成；没有字段归一化，只有宿主时间显示归一化 TIME/ZERO。

只读来源 `operator.go` Cancel/Replace/Sync/SchedulerKind/History/Record/String/
MarshalJSON/ToJSONObject。相邻 `TestOperator` 验证 JSON 字符串长度，TestSchedulerKind
提供14条 kind 案例，另加零位图；TestRecord 覆盖未启动记录，TestToJSONObject 覆盖
对象字段及成功/超时；TestAdjustStepCostUsesRegionKeys、TestPeerInfluenceUsesRegionKeys、
TestOperatorCheckConcurrently、TestOperatorStoreHealthCheck 属于前置影响量/推进/健康
契约，不将其名字当作本任务运行覆盖。Go Operator 没有完成回调；FinishedCounters
由后续 Controller 消费，本次并发终态转换测试证明只允许一个成功转换，没有增加
另一套回调机制或将计数器执行冒充已实现。

[task_025_test.rs](../../../pkg/schedule/operator/task_025_test.rs) 显式注册5项真实测试：

- `task_025_behavior_cases`：15组 scheduler kind；Cancel 首原因、多参数、无原因再补、
  终态拒绝；Created/Started Replace；双向关联 merge 及 timeout 同步。
- `task_025_history_record_and_output`：3条历史，leader 优先及不平衡配对；取消后
  记录从前一步计时；成功记录、负时长、未启动零时间错误及宿主解析；operator/record
  字符串 JSON 往返、状态及步骤输出。
- `task_025_terminal_once_and_metadata_race`：32线程竞争 Cancel/Replace，恰好一次
  转换成功、终态时间不变；实际元数据日志转义。
- `task_025_output_boundaries`：逐字段完整对象、完整 normalized String、精确60秒
  超时及后缀；成功记录仍从 start 计算、成功后拒绝取消；14个原因字符串；epoch
  零字段省略以及 Go 直接引号包装产生非法 JSON 的边界。

- `task_025_duration_saturation`：真实记录跨越纳秒 i64 范围时正向饱和 MAX、
  负向饱和 MIN，精确核对负极值字符串。

表驱动核心为15组 kind、3条历史、14种原因，共32条固定预期；另有上述状态与
时间故障边界，全部无忽略。

无外部 I/O 或持久化调用，不注入不适用的 MetaStore/mock。状态拒绝、重复终态、
并发竞争、零时间错误和非法 JSON 为本范围的边界/故障证据。时间由真实 Clock 边界
控制；Record 保留未达到的时间为 None，墙上时间由宿主映射，避免伪造时间戳。

## 验证

[red-missing.out](logs/task_025/red-missing.out)：先注册测试，缺失 API 编译失败，退出101。
随后实现完成，用 [red-behavior.py](logs/task_025/red-behavior.py) 注入 scheduler kind
错误、空 history、零 record duration、取消总报告成功，最终5项测试均断言失败，退出101
（[red-behavior.out](logs/task_025/red-behavior.out)、[red.json](logs/task_025/red.json)）。
脚本 finally 恢复原生产字节并格式化；不是仅靠缺包/缺注册失败。

最终确切命令、退出码和有效测试数保存在 [validation.json](logs/task_025/validation.json)。
共享槽位由 [slot-run.py](logs/task_025/slot-run.py) 原子 mkdir 动态领取，正常/失败/信号
退出先等待子进程再释放，只删除自己的 owner/锁，保留缓存。
workspace 的既有 etcd I/O 测试使用前置已保存二进制，复用及哈希见
[provenance](logs/task_025/fixture-reuse-provenance.json)。Go 拦截器拒绝所有实际 Go
调用，仅复制这份已有外部 fixture，不作为本任务 Go operator oracle。


| 最终命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_025 -- --nocapture` | 0 | 5通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 40通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1403通过、0失败、1既有忽略 |

32条表驱动核心预期全部成功、0失败。最后的变异红测还覆盖极值时长测试，5项
均失败（退出101）；finally恢复后的生产字节哈希记录于red.json，与通过workspace
验证时的生产代码一致；恢复后再次聚焦运行5项全通过，见
[green.out](logs/task_025/green.out)。

所有阶段实际动态领取槽位 **1**，绝对路径
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
验证结束已释放自有锁，保留编译缓存。[verification.json](logs/task_025/verification.json)
核对1119条只读文件清单，其中1101个Go/模块文件内容不变，整体和包级plan.md未变，
`git diff --check`退出0；fixture SHA256为
`86fd780eefafe63a7891ee03151370f1cb15fc8c4ce4aa8b3e5cff819171da26`。
已审查格式化与最终差异，仅本任务Rust/API接线、测试、说明和证据纳入提交；
既有任务10–24删除不纳入。本任务编号文件完成后删除，稳定证据保留供任务26读取。

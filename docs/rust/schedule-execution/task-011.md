# 任务 011：过滤计数刷新与清理

状态：已完成。批次10提交 `08a268883` 的生产 API、45组运行 oracle、22项包测试、
workspace 基线及 [task-010](task-010.md) 已核实。本任务串行执行，未修改两级计划
或来源主题。新增 [生产实现](../../../pkg/schedule/filter/filter_metrics.rs)、
[领域说明](../../../pkg/schedule/filter/filter_metrics.rs.md) 与
[独立测试](../../../pkg/schedule/filter/task_011_test.rs)，只扩展 canonical filter crate。

## 生产边界

`FilterCounter::{new,inc,set_scope,pending,flush}` 迁移按 action/type/store 分组的
批次；flush 先消费计数，再查询真实 `schedule-core::BasicCluster`，跳过 missing/
Removed store。None 对应 nil informer。`FilterType::as_str` 保留26个名称及上界
unknown；负数保留 Go panic。`FilterAction` 表示 source/target 两种有效方向。

`FilterMetrics::{register,delete_store_metrics,reset_filter_metrics}` 使用宿主 Registry，
保留 `pd_schedule_source_filter` / `pd_schedule_target_filter`、原 help、Counter 类型和
`scope,type,source|target` 标签。缓存句柄随删除/reset 一同清理；生命周期锁协调共享
clone 的刷新、删除、reset。删除覆盖所有 scope/type，重复删除安全，其他 store 保留。
注册冲突返回错误，第二个向量失败时回滚本次第一个向量。inc 不操作 Prometheus。

仅复用 workspace 的单一 prometheus 和 schedule-core；Cargo.lock 只增加这两条既有
依赖边，最终无远程依赖版本变化。无进程全局指标、runtime、协议类型或持久化路径。

## Go 冻结与对照

来源 commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[provenance](logs/task_011/provenance.json) 保存来源源码、二进制及 fixture SHA256。
[freeze.py](logs/task_011/freeze.py) 复用既有 `/tmp/pd-sched003/cluster.test` 和 Delve，
在 TestSetOfflineStore 完成6个真实 store 注册后，只改变 scope/NodeState 领域输入，
调用真实 Counter/指标方法，冻结 [10组 fixture](fixtures/task_011.json)。
[命令](logs/task_011/oracle.commands) 和 [逐命令输出](logs/task_011/oracle.json)
保存 argv、stdout、exit code 0；不声称修改输入后的整个 Go 测试通过。

| 场景 | 实际观察 |
| --- | --- |
| pending | source store1/target store2 各1；尚无指标 |
| flush | source累计2，target累计1；pending清零 |
| empty_flush | 再次刷新不增加值、不创建新序列 |
| scope | 已计数 source 随新 scope 刷新，旧 scope 指标保留 |
| delete | store1 两个 scope、两个方向全部删除；store2保留 |
| delete_again | 重复删除及未知store无副作用 |
| reset | 两个方向全部清空 |
| live_and_missing | 真实store1/2有指标，missing99无序列；pending全消费 |
| buried_before_flush | 计数后将store1设Removed并删除指标；source/target均不复活，missing target99不计数 |
| no_resurrection | 再用nil informer刷新，也不重放已丢弃计数 |

SetScope 没有 Go 调用者，被已有二进制链接器消除；只读确认其单行 scope 赋值，
调试器设置同一 scope 输入后冻结真实 Flush 输出，明确区分 setter 源码契约和运行
指标结果。Go 使用全局向量，Rust 的独立 Registry、共享 clone、注册回滚和线程测试
属于宿主嵌入契约，不冒充 Go 独立 Registry 运行差分。

[safe-tests](logs/task_011/safe-tests.json) 另外完整运行现有 TestString、TestCounter，
退出0，2项通过：前者实际断言 tombstone、最后一个filter、越界unknown，后者断言
两个方向计数和flush清零。其余边界来自上述真实方法调用，不以测试名代替覆盖。
已检查 TestMain/WaitForEtcdConnections 和所选 setup/方法调用链：无需 failpoint
注入，未运行任何 Go 编译、make、生成器、格式化或 failpoint 启停命令。

归一化仅排序map/序列，并排除调试地址、map hash、锁、内部descriptor ID、protobuf
cache及未导出的created时间戳。scope/type/store/方向/数值、pending、指标元数据全部
原样比较。调试器的 chained GetStore 赋值探测不可靠，最终通过 canonical map 输入
设置 Removed 并保存实际NodeState输出；[second-pass](logs/task_011/second-pass.out)
是修正该 oracle 输入前的诊断失败，不列为有效 TDD 行为证据。

## TDD 与故障证据

先注册测试，取得 [red-compile](logs/task_011/red-compile.out)，退出101，缺少API和
依赖；该编译失败不充当行为断言证据。[first-pass](logs/task_011/first-pass.out)
记录测试抓取器 `get_value` API 修正前的编译诊断。

[mutation.py](logs/task_011/mutation.py) 对同一注册测试独立注入并恢复10个错误实现：
重复flush、忽略scope、Removed复活、missing计数、跳过target删除、漏删某scope、
漏reset source、reset保留旧缓存句柄、注册失败泄漏source、target写进source。
[mutation结果](logs/task_011/mutation.json) 全部退出101，分别有
4/1/2/2/2/1/1/1/1/2项真实行为测试失败，没有编译失败冒充断言。
finally 恢复源码并格式化；最终验证针对恢复后的源码。

5项有效测试包含10组冻结场景（10通过、0失败）、26个filter名称（26通过、0失败）；另验证u64最大store ID、删除/reset后
缓存重新创建、精确十进制label、Registry隔离/注册回滚、负索引panic，以及4线程
各100个source/target批次共享指标（8条序列均100，删除后全部消失）。Removed/missing
和冲突注册是此无I/O组件的故障边界，无无关存储mock。

## 最终验证

[validation.py](logs/task_011/validation.py)、[结果](logs/task_011/validation.json)
和各 `validation-N.out` 保存确切命令、退出码与有效测试数。

| 命令 | 退出码 | 有效测试 |
| --- | ---: | ---: |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test -p astersql-pd-schedule-filter --locked --lib task_011 -- --nocapture` | 0 | 5通过，0失败/忽略 |
| `cargo test -p astersql-pd-schedule-filter --locked` | 0 | 27通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace编译 |
| `cargo test --workspace --locked` | 0 | 1353通过，0失败，1既有忽略 |

[slot-run.py](logs/task_011/slot-run.py) 每阶段原子mkdir从1–10动态领取槽位，记录
plan/task/session/PID；所有Cargo和子进程继承绝对 CARGO_TARGET_DIR。各阶段实际槽位1：
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
正常、失败、信号路径等待子进程结束再释放自己的锁，缓存保留；
[最终阶段输出](logs/task_011/validation-stage.out) 保存领取与结束记录。

[verify.py](logs/task_011/verify.py) 重新解析原始Go输出核对fixture，校验二进制/
源码哈希、断言失败、全部验证测试数、依赖方向、远程依赖版本、相对链接、Go/模块
起止内容及状态（含untracked/ignored）、全部只读plan.md和本任务锁清理。
[审计结论](logs/task_011/evidence-verification.json) 与
[Go核对](logs/task_011/go-verification.json) 保存实际数量。
`git diff --check` 通过。只提交任务11实现、接线、测试及证据，保留初始任务10删除，
不提交该无关变更。完成后删除任务11编号文件，两级plan及原主题保持原样。

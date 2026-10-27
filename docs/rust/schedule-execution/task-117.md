# 任务117：明确外部插件不支持边界

前置116提交 `c62630aa5` 及 [证据](task-116.md) 已核实：热点查询和观测API存在，
6项聚焦、26项包级、1770项workspace验证通过。本批次只执行117。
两份plan.md保持只读，无并行任务。

用户后续明确授权以**只读Go源码/测试推导 + Rust真实行为和故障验证**替代缺失的
Go运行oracle。[provenance](logs/task_117/provenance.json) 保留来源commit、
checkout commit、源文件和fixture SHA-256，以及与固定来源源码的逐文件比较。
Go运行命令、二进制、输出均为null。已审查oracle-index，其运行案例未覆盖本次
plugin边界；不伪造运行差分，也不把Go加载成功语义说成不支持。

只读来源：`plugin_interface.go::GetFunction` 执行绝对路径解析、plugin.Open和Lookup；
`coordinator.go::LoadPlugin`取得SchedulerType/SchedulerArgs后创建并添加scheduler，
并启动waitPluginUnload；Unload信号删除scheduler。错误分支返回，无后续注册。
Rust目标依据ADR-001与本任务要求，明确拒绝Go ABI请求，而不是迁移Go加载流程。

生产路径：[plugin_boundary.rs](../../../pkg/schedule/plugin_boundary.rs)，
[领域说明](../../../pkg/schedule/plugin_boundary.rs.md)。公开API：
`Coordinator::load_plugin(&Path)`、`Coordinator::unload_plugin(&Path)`，均返回
`Result<(), UnsupportedGoPlugin>`；错误保留`PluginAction::{Load,Unload}`，
固定文本`Go scheduler plugins are not supported`。方法体直接拒绝，不读取文件，
不转换/验证路径、不取得组件锁、不访问持久化、不创建任务；无新增依赖。

[fixture](fixtures/task_117.json) 的输入与期望独立固定，不从Rust输出生成。
正常、边界与故障案例：5种路径（空、相对缺失、绝对缺失、内置同名、父目录相对）
各执行Load/Unload，共10项表驱动请求；另对已有非plugin Go源文件和NUL非法路径
执行两种请求，共4项。14项请求均应明确拒绝，无字段归一化。

2项显式注册的真实测试：

- `task_117_behavior_cases`：真实Coordinator、BasicCluster、CheckerDispatcher、
  SchedulerController及MemoryStore；启动内置balance-leader后暂停，再请求插件。
  每次核对错误类型/操作/文本、内置名称和配置、暂停状态、初始化状态、近期晋升
  checker状态及cluster/checker/operator owner身份；核对MemoryStore完整snapshot
  （revision、entries、more）保持一致。请求期间持有真实checker锁，证明不会重入。
- `task_117_existing_file_and_invalid_path`：未初始化门面亦明确拒绝；已有文件内容
  保持一致，NUL路径不会变成文件系统错误；无新增scheduler且初始化状态不变。
  无外部I/O注入框架：两个生产方法没有I/O路径，非法输入就是故障边界。

[初始日志](logs/task_117/red.json) 保存测试开发时Read API参数和Snapshot比较的
编译错误，不作为行为红灯。[行为红灯](logs/task_117/behavior-red.json) 在可编译
但错误返回Ok的实现上运行相同2项测试，exit101、0通过/2失败，断言明确指出
Ok与UnsupportedGoPlugin不符。随后最小实现直接返回Err，测试保留并通过。

[最终日志](logs/task_117/final.json) 保存精确命令、退出码、共享槽位和压缩stdout/stderr。
各阶段均从1到10原子mkdir动态领取槽位；CARGO_TARGET_DIR使用仓库绝对路径，
设置CARGO_BUILD_JOBS=1、RUST_TEST_THREADS=2、GOFLAGS=-mod=readonly并继承到子进程。
Rust修改后先cargo fmt --all，再聚焦、包级及完整workspace验证。
已有etcd测试fixture只构建现有只读Go工具，输出和WAL在槽位；调用链已核查，
不注入failpoints，不执行Go测试，不运行Go格式化/模块整理。proto命中既有缓存。

[首次通过尝试](logs/task_117/pre-final.json) 有1项通过/1项失败：测试装配采用
默认0ns witness间隔，近期晋升状态已过期。已在测试装配明确设置10ns并增加
请求前断言，生产plugin实现无需变更。最终运行采用修正后的真实checker状态。

最终结果与只读复核见下方追加记录。

最终7条命令全部exit0：[测试统计](logs/task_117/test-counts.json) 为聚焦2通过、
包级28通过、workspace1772通过/0失败、1项既有ignored（含2项compile-fail文档
测试通过）。14项边界请求全部通过。metadata、fmt check、workspace check及
修改后的cargo fmt均退出0，未产生本任务范围外格式化差异。

全部Cargo阶段实际动态领取槽位1，绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[最终复核](logs/task_117/verification.json) 确认自有owner记录和锁目录已清理，
无使用槽位产物的进程，编译缓存保留。
[只读基线](logs/task_117/readonly-before.json.gz) 与结束时重新枚举结果比较：
1101份Go/模块文件清单、SHA-256、Git状态完全相同，两份plan.md哈希相同；
`git diff --check` exit0，相关Markdown链接有效。没有修改Cargo.lock或Go文件。
仅提交117的实现、测试、局部注册和证据，排除既有其它编号任务删除记录。

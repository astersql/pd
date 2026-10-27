# 任务124：加速复查与规则查询

依赖123为提交 `cda8aba2a`，其[证据](task-123.md)记录6项聚焦测试、30项包测试、
1802项workspace测试通过。canonical Coordinator/Handler、RegionsInfo、checker队列、
RuleManager及filter复制判定均存在，已实际复用；没有依据任务文件删除推断完成。
整体及包级plan.md保持只读。

用户明确授权以只读Go源码/测试推导与Rust真实行为/故障验证替代缺失的Go运行oracle。
本任务的oracle-index条目只有source_assertion，没有冻结运行输出；没有运行Go测试、
Go oracle或failpoint工具，也没有声称Go运行差分。
[provenance](logs/task_124/provenance.json)保存source_commit、前置提交、来源源码及
fixture SHA-256。Go二进制和输出为null。handler.go与调用方region_test.go均与来源
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`逐字节一致。
参考handler.go:1113–1186、1274–1354及region_test.go的
TestAccelerateRegionsScheduleInRanges、TestCheckRegionsReplicated；后者包含failpoint
路径，未裸跑。只读源码和Rust运行证据明确分开。

生产路径是[region_admin.rs](../../../pkg/schedule/handler/region_admin.rs)，API为
Handler::accelerate_regions_schedule_in_range/in_ranges、adjust_limit、
check_regions_replicated、check_region_placement_rule，以及独立adjust_region_limit。
PreCheckForRegion复用已有Handler::pre_check_for_region，无重复实现。
[领域说明](../../../pkg/schedule/handler/region_admin.rs.md)记录宿主持有的规则、时钟、
当前stores/config边界，以及领域错误与传输映射。只扩展已有canonical crate，没有
server/proto依赖、复制状态、私有runtime或mock。

[fixture](fixtures/task_124.json)的7行limit预期按Go常量与解析规则冻结；其它独立
字面预期保存在[测试](../../../pkg/schedule/handler/task_124_test.rs)，不从待测实现生成。
ID、状态与错误没有忽略；队列按集合排序观察，其它字段没有归一化。

| 实际测试 | 行为与边界 |
| --- | --- |
| task_124_limit_cases | 7行正常limit、首个默认值、7行非法/溢出输入；默认16、上限10240、符号、前导零、0/负数/i64最小值 |
| task_124_behavior_cases | 单范围限1、多范围逐范围限1及重叠去重、非正全量，另4行包含点/有限end/空end/空范围/全范围扫描 |
| task_124_replication_and_rules | 传统/placement两模式、精确副本数、REPLICATED/INPROGRESS、加速队列与rule pendingList分离、实际规则拟合、禁用错误、原始Arc身份；真实checker因无候选store进入PENDING；持久化修改规则后变REPLICATED |
| task_124_errors_and_precedence | 7行非法ID、3行不存在ID（含u64最大值）、hex错误及精确文本、奇数长度、大小写hex、空多范围、长度不匹配无副作用、未初始化与解析错误先后顺序，三个新查询入口bootstrap错误 |
| task_124_acceleration_bypasses_retry_cap | 真实TTL队列预填100002条；加速仍加入region1，不误写rule pendingList |

故障证据使用真实规则检查器和耗尽候选stores的永久拒绝。只对宿主外部状态进行
显式控制；规则变更通过实际MemoryStore/StorageEndpoint/RuleManager持久化，查询
随后观察新规则。handler这些入口本身无持久化I/O，没有编造查询存储故障。TTL后台
清理均等待shutdown。PreCheck的400/404/500由已存在的类型化错误表达，HTTP装配不在范围。

[initial](logs/task_124/initial.json)记录新增测试后缺失API的exit101；
[first](logs/task_124/first.json)为API接线编译诊断，不能视为行为红灯。
[corrected](logs/task_124/corrected.json)记录新增3项现有workspace测试依赖需要刷新锁；
离线generate-lockfile后恢复无关pulldown-cmark版本，最终Cargo.lock仅增加这3项路径依赖。
[behavior](logs/task_124/behavior.json)暴露测试装配缺少replica_schedule_limit和make-up
开关，按真实checker前置条件补全后[dispatch](logs/task_124/dispatch.json)4项通过，
[expanded](logs/task_124/expanded.json)5项通过；前述装配错误不冒充生产行为红灯。

[behavior-red](logs/task_124/behavior-red.json)记录相同完整测试对故意错误实现的检测：
把min cap改为max、false容量检查改为true、PENDING改为INPROGRESS，exit101，
2通过/3失败；分别实际断言10240不等于16、INPROGRESS不等于PENDING、队列缺region1。
随后恢复正确实现，重新执行格式化及全部必需验证。

[final.json](logs/task_124/final.json)保存确切命令、退出码、实际test result和原始gzip
日志哈希。每个阶段均由[verify.py](logs/task_124/verify.py)原子mkdir动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
持有者记录包含任务/计划/会话/PID；正常、异常和中断时等待自己的子进程结束再释放锁，
保留缓存。全部Cargo命令及子进程继承此绝对路径；CARGO_BUILD_JOBS=4、
RUST_TEST_THREADS=4、GOFLAGS=-mod=readonly。

最终验证全部退出0：

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化完成 |
| `cargo test -p astersql-pd-schedule-handler --locked --lib task_124 -- --nocapture` | 0 | 5通过、0失败、0忽略 |
| `cargo test -p astersql-pd-schedule-handler --locked` | 0 | 35通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace装配通过 |
| `cargo fmt --all -- --check` | 0 | 格式检查通过 |
| `cargo check --workspace --locked` | 0 | workspace检查通过 |
| `cargo test --workspace --locked` | 0 | 1807通过、0失败、1既有忽略 |

workspace计数含2项compile-fail文档测试。唯一忽略项为既有测试；没有新增忽略。
只有既有server deprecated字段warning。
[最终只读复核](logs/task_124/verification.json)：1101份Go/模块文件（含未跟踪路径）
与起始路径及内容哈希一致，Git无Go/模块差异；两份plan.md哈希不变。
git diff --check通过，文档相对链接已核实。自己的槽位锁已释放，缓存保留。
仅本任务实现、测试、依赖接线、说明和验证证据纳入提交，其它任务已有删除保留。
状态：已完成。

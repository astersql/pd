# 任务118：操作查询、删除与记录接口

已核实前置117提交 `47d0e8839`、[证据](task-117.md) 和真实Coordinator API。
117有2项聚焦、28项包级、1772项workspace通过证据。当前仅执行118。
整体/包级plan.md只读。遵循用户后续授权，以只读Go源码/测试推导及Rust真实
行为和故障验证替代缺失Go运行oracle；不声称Go运行差分。

[provenance](logs/task_118/provenance.json) 冻结来源commit、checkout commit、
源码及fixture SHA-256；Go命令、二进制、输出均为null。oracle-index的既有运行
案例不覆盖本次Handler故事。只读审查 `handler.go:75–240` 的接口/错误分支，
`operator_controller.go` 的删除/查询/归档行为，`operator_controller_test.go`
及 `tests/server/api/operator_test.go::checkRemoveOperators`。后者验证删除全部
运行操作，不能当本次已执行的Go证据。

生产路径：[operator_query.rs](../../../pkg/schedule/handler/operator_query.rs)，
[领域说明](../../../pkg/schedule/handler/operator_query.rs.md)。新canonical crate
`astersql-pd-schedule-handler`，只使用领域输入输出，不依赖server、HTTP或proto。
Handler借用真实Coordinator的admission/records/cluster/config，不复制所有者。
ControllerAdmission仅增加等待队列只读快照。根workspace及Cargo.lock注册本包，
保留所有远程依赖原版本。

公开API包括控制器/单操作/状态/所有运行操作/等待操作/Admin/Leader/Region查询、
按kinds拼接、单个/全部删除、history和records。错误类型保留未初始化与不存在
区别及Go RFC code。records返回拥有canonical Arc的ArchivedOperator，record()
提供已有OpRecord领域视图。删除组合AdminStop、实际移除成员的限流ack和归档；
等待队列保留，关联merge成员也归档。时钟显式传入；不做元数据I/O。

[fixture](fixtures/task_118.json) 独立冻结12项kinds输入与期望（含组合/重复/未知），
仅对无序group排序ID，保留重复。其它测试断言直接对应只读源码契约。无时间字段
忽略；Instant是明确控制的领域输入，无wall-clock转换或运行输出伪装。

4项显式注册测试：

- `task_118_behavior_cases`：真实Coordinator/BasicCluster/ControllerAdmission/
  ControllerRecords/MemoryStore；12项fixture表驱动case；验证canonical Arc身份、
  STARTED/CANCELED状态、查询/删除不存在错误、空history/records区别、AdminStop、
  重复删除、inclusive时间过滤、等待队列保留/晋升、active覆盖旧记录、同region
  新记录替换旧记录、600秒TTL的direct/enumeration边界及过期、无元数据写入。
- `task_118_uninitialized_errors`：16项实际API错误路径均为NotBootstrapped；
  空/未知kind的3项列表无需coordinator，错误code稳定。
- `task_118_merge_removal_and_kind_masks`：空查询/RemoveAll、混合运行集合，按完整
  kind mask筛选，关联移除、主/关联cancel-reason和counts、3项终态记录/history；
  已SUCCESS操作被管理删除后保留成功状态。
- `task_118_removal_acknowledges_store_limit`：真实SlidingWindows V2和AddLearner
  成本，单删/全删两条路径的占用均96→0；不存在删除返回错误且占用96不变。
  不使用mock，不对无I/O的管理API增加人工持久化故障开关。

失败记录：`initial.json`是测试装配Read API编译错误，不作行为红灯；
`behavior-red.json`是默认等待限额0导致的装配失败；修正限额后
`behavior-red-fixed.json`已捕获leader查询错误（返回admin集合），同时发现admin
操作默认为urgent，测试需显式low才能排队更高优先级替换。最终红灯与通过结果
在下方最终验证记录中列出。保留失败日志，不将装配问题当成功证据。

全部Cargo阶段由1至10原子mkdir动态领取槽位，owner记录包含计划/任务/会话/PID，
绝对CARGO_TARGET_DIR传递所有子进程。CARGO_BUILD_JOBS=1、RUST_TEST_THREADS=2、
GOFLAGS=-mod=readonly。每阶段结束后删除自有owner并rmdir，保留缓存。
源码修改后先cargo fmt --all。workspace已有etcd测试只编译已有只读Go fixture，
二进制与数据目录位于槽位，已审查调用链不注入failpoint、不写Go/模块文件；
proto生成入口只读固定Go模块ZIP，在Cargo OUT_DIR生成协议。禁止执行Go测试或
Go格式化/整理/注入命令，最终重新枚举和哈希全部Go/模块文件。

最终行为红灯 [red-verified.json](logs/task_118/red-verified.json)：在其余装配正确后，
将leader过滤错误地指向Admin，4项真实测试3通过/1失败，exit101；失败断言明确为
运行leader ID列表 `[] != [4]`。同一测试随后以Leader mask恢复通过。此前
red-final还暴露测试区域approximate_size默认0，已明确设96以产生真实snapshot成本，
生产限流逻辑不需修改。所有红灯日志保留供审查。

最终 [final.json](logs/task_118/final.json) 保存7条确切命令、退出码、测试统计和
压缩原始stdout/stderr日志：cargo fmt、聚焦测试（4通过）、包级测试（4通过）、
metadata、fmt check、workspace check、workspace test全部exit0。
workspace共1776通过、0失败、1项既有ignored，含2项compile-fail文档测试。
12项表驱动kinds案例、16项未初始化错误路径、2项实际限流释放路径全部通过；
无过滤或忽略本任务测试。

各阶段实际动态领取槽位1，绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[最终复核](logs/task_118/verification.json) 确认自有锁释放，无使用槽位产物的进程，
保留编译缓存。重新枚举1101份Go/模块文件与开始时清单、SHA-256、Git状态完全
相同，两份plan.md哈希相同；git diff --check exit0，相关Markdown链接有效。
只提交118的生产实现/测试/Cargo注册/说明/证据；排除既有其它编号任务删除。

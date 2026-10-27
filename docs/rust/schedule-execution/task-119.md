# 任务119：领导者与副本管理操作

批次119依赖118已满足：提交 `09286e2e0`、[118证据](task-118.md)、
canonical Handler/Coordinator/ControllerAdmission/records API均已核实。
整体/包级 plan.md 保持只读。用户后续明确授权以只读Go源码/测试推导及Rust真实
行为和故障验证替代缺失Go运行oracle；本任务不声称取得Go运行差分。

[provenance](logs/task_119/provenance.json)冻结来源commit、checkout commit、
Go来源/fixture SHA-256与该授权。oracle-index已有运行案例不覆盖本故事；Go运行
命令、二进制及输出均为null。只读审查 `handler.go:408–578,720–745,1324–1341`、
`operator/create_operator.go`、`operator/builder.go`，以及
`tests/server/api/operator_test.go::checkAddRemovePeer/checkTransferRegionWithPlacementRule`。
后两项仅作源码推导，未运行其Go测试。

生产路径：[admin_peer.rs](../../../pkg/schedule/handler/admin_peer.rs)、
[说明](../../../pkg/schedule/handler/admin_peer.rs.md)。公开领域API为
`Handler::add_peer_operator`、`AdminPeerRequest` 六个动作、`AdminPeerContext`、
`AdminPeerError`、`Handler::pre_check_for_region`。复用现有PeerOperatorFactory的
普通/联合共识规划、真实Coordinator config/cluster/admission与移除归档/ack，
无server/proto/HTTP依赖。前置模块仅扩大两个内部方法可见性供同包复用。
新增placement直接依赖；Cargo.lock仅新增该路径依赖，离线生成时发生的无关
pulldown-cmark升级已排除，所有远程版本保持不变。

[fixture](fixtures/task_119.json)的6项正常案例固定region/peers/leader/allocator和
步骤序列；详细目标、ID、kind、URGENT优先级、STARTED状态由真实对象验证。
TransferRegion角色省略仅在placement关闭时补voter；TransferPeer保留role/witness。
显式传入源map迭代顺序、时钟、规则及集群版本能力，不忽略错误或步骤字段。
PreCheck严格使用ASCII十进制（包括前导零），拒绝正负号、空白、下划线和溢出。

5项独立注册真实行为测试：

- `task_119_behavior_cases`：6个fixture动作，逐步骤/目标/peer ID/kind/优先级/运行
  状态及空等待队列断言。
- `task_119_validation_and_faults`：7项请求校验错误、真实allocator I/O错误、removed
  store、601秒失联store、重复URGENT admission拒绝且原Arc不变；错误均不入队。
  PreCheck覆盖10项输入，包括不存在region与十进制解析边界。
- `task_119_unbootstrapped`：PreCheck及6个动作共7项未初始化错误，校验次序保持一致。
- `task_119_roles`：空目标、仅learner、未知角色、多个leader共4项builder错误；
  placement开启时缺角色、缺适用规则共2项错误，均不入队。
- `task_119_preserves_peer_roles_and_placement`：learner/witness两条peer迁移路径保留
  原角色与witness；禁止将其设为leader。真实RuleManager通过MemoryStore初始化默认
  规则并取得apply-region规则，显式leader/follower/voter迁移成功。

共43个表驱动或独立场景（6+11+10+7+6+3），5项测试无ignored。
所有harness使用真实BasicCluster/Coordinator/ControllerAdmission/PeerOperatorFactory；
只控制host时钟、能力与allocator边界。Handler不做元数据写入，无人工持久化故障开关。

失败证据：initial为锁文件需新增路径依赖，compile为内部方法可见性缺失，均不当作
行为红灯。[behavior-red](logs/task_119/behavior-red.json)将AddLearner错误地当voter，
真实测试exit101（3通过/1失败）：实际`[add,promote]`与冻结期望`[add]`不符；随后
恢复正确角色同一测试通过。PreCheck后续源码复核纠正Rust与Go前导加号差异，
对应行为红灯与最终通过命令另存日志。

所有Cargo阶段动态原子mkdir领取槽位1，绝对
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录包含计划/任务/会话/PID，阶段退出清理自己的锁并保留缓存。
首轮基线使用CARGO_BUILD_JOBS=1、RUST_TEST_THREADS=2；最终复核使用
CARGO_BUILD_JOBS=4、RUST_TEST_THREADS=4；均设置GOFLAGS=-mod=readonly。
workspace既有etcd fixture仅编译已有Go工具到槽位，proto入口只读固定模块ZIP，
未运行failpoint注入/格式化/tidy等Go改写命令。

PreCheck实际行为红灯：[precheck-real-red](logs/task_119/precheck-real-red.json)
为5项测试4通过/1失败，exit101；输入`+1`实际成功，而只读Go ParseUint源码推导
要求InvalidRegionId。移除错误的前导`+`剥离逻辑后，同一断言通过。
`precheck-red`仅是中间测试编辑记录（尚未插入`+1`无效输入），不算红灯证据。

最终[verified.json](logs/task_119/verified.json)保存7条确切命令、退出码和压缩原始
stdout/stderr日志及其SHA-256：cargo fmt、聚焦测试、包级测试、metadata、fmt check、
workspace check、workspace test全部exit0。聚焦5通过，包级9通过；workspace
1781通过、0失败、1项既有ignored，含2项compile-fail文档测试；本任务没有忽略测试。
首轮[基线](logs/task_119/final.json)同样1781通过，最终复核发生在PreCheck修正之后。

[最终只读复核](logs/task_119/verification.json)：1101份Go/模块文件路径及SHA-256
与任务开始时完全一致；Git无Go/模块差异；两份plan.md哈希不变；git diff --check
exit0。自有槽位1锁已释放，槽位产物无遗留运行进程，编译缓存保留。
仅提交119相关生产实现、测试、Cargo接线和证据，排除其它编号任务已有删除。

状态：已完成。

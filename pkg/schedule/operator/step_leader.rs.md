# TransferLeader 领域步骤

`TransferLeader` 对应只读 Go `step.go` 的同名步骤：`is_finish`、
`conf_ver_changed`、`check_in_progress`、`influence`、`timeout`、`get_cmd`。
复用 core 的 RegionInfo/BasicCluster/Peer、已验证的 OpInfluence、配置 provider
及 hbstream 的领域 Operation；不依赖 protobuf、网络、持久化或运行时。

完成只依赖新 heartbeat 报告的 leader，任意候选或主目标均可确认；旧 leader
不会因为发送命令而变化。无 leader 的 store ID 为0，保留 Go nil getter 行为。
检查按 ToStores 后 ToStore 顺序，任一有非 learner peer 且 store 有效的候选成功
即返回；全部失败保留顺序、重复项和每项 store ID 的类型化错误。
健康检查为调用者显式开关，Down 检查始终执行；时钟由调用者传入 Unix 纳秒，
使用 core 的严格 `>10min` 和 provider 的有符号最大 down 时长。
不额外检查 witness、pending/down peer 列表或非 learner 联合角色。
候选列表不可变，重复检查没有 Go backing array 别名风险。

资源影响只从 FromStore 转到 ToStore；每个不同候选（包括主目标）预留1000
TransferLeaderIn 成本，候选缺 peer 也预留；源和主目标相同时资源抵消。
有符号资源运算保留 Go wrapping；更新使用已有 OpInfluence 的逐 store 锁，
与 Go 一样不承诺跨 store 原子快照。

超时以浮点0.6乘大小，先截断秒，再 wrapping 转为纳秒，最低60秒；返回的
std::time::Duration 始终非负。命令保存主 peer 的缺失值、候选顺序、重复及
缺失项；useConfChangeV2 不改变此命令。生成命令不隐式执行健康检查。

依据是冻结的只读 Go 源码与测试契约（用户已解除运行 oracle 门槛）；
未执行 Go operator，也不宣称 Go/Rust 运行差分。详细来源、红绿与验证见
[task-018](../../../docs/rust/schedule-execution/task-018.md) 和其 fixture。
测试在独立 `task_018_test.rs` 显式注册，使用真实领域对象；没有 mock 或 I/O。

# 任务038：键空间配置前置条件

状态：已完成，2026-11-05，基准a653895ed。
[task-038-evidence.json.gz](task-038-evidence.json.gz)保存原始阶段、Go逐例/在线差分、真实物理状态及metadata。

KeyspaceManager增加有序Put/Delete与带前置条件的更新：null要求不存在、空字符串要求存在且为空，
不匹配返回精确ErrKeyspaceConfigPreconditionFailed；未知操作返回ErrIllegalOperation。
名称和metadata用同一revision事务读取，只有Enabled/Disabled可以更新；删除不存在的字段幂等。
响应端点字段在保存前清除，提交后通过27/28的原manager装饰返回值，不写回持久配置。
metadata与meta-service assignment计数同事务，目录读锁覆盖校验到提交；unknown/disabled目标在TSO移动前拒绝。
复用30的唯一成员迁移实现，拆出内部持锁入口，保持membership→meta-group directory→metadata的锁序，
不复制迁移算法；放宽27内部rollback借用生命周期，不改变其公开行为。

只读来源keyspace.go:839–1023、meta_service_group.go:219–301以及相邻既有测试。
前置条件和配置事务失败保持metadata/计数原状；Go并不提供跨独立TSO事务的全局回滚：
Go会先独立提交TSO成员迁移，再提交配置事务。实际2MiB更新被gRPC大小上限拒绝时，
原metadata仍指向group1，但group1已移除、group2已持久加入该键空间。
Go同WAL重启后这一状态仍存在。Rust保留并逐项断言相同阶段边界，未声称所有分组状态均回滚。
本项不改写Go，也不把观察到的部分迁移隐藏在归一化中。

验证：

- 可运行red 1项真实etcd：配置patch未提交导致断言失败，exit101；实现后通过。
- 聚焦5项真实etcd通过，完整keyspace包79项通过、0失败/0忽略。
  覆盖absent/空字符串/相等与不等、顺序put/delete、非法操作、ARCHIVED/TOMBSTONE/未知状态、
  独立manager CAS单赢家、meta分组校验先于TSO迁移、成功计数与端点装饰、实际过大请求拒绝及WAL恢复。
  故障后原metadata bytes/mod revision、meta计数不变；独立TSO迁移按Go保留。
- 同源既有Go TestMetaServiceGroupTestSuite/TestReassignRejectsDisabledGroup实际执行1个叶例通过；
  只读sandbox同时拒绝网络和源码/SDK/cache写入，未裸跑依赖skipSplitRegion的keyspace主suite。
- 两项显式--ignored在线差分实际执行通过：正常模式八组HTTP patch/precondition输入逐项比较，
  成功DTO、409前置条件消息、400 unknown group消息完全相等；失败物理bytes和mod revision不变，
  成功及空patch均推进metadata mod revision。保存前后原始键、bytes、create/mod revision。
- API模式真实HTTP过大PATCH由文件传入curl，避免shell参数大小限制；实际Go返回HTTP500，
  原始ResourceExhausted消息及字节大小记录。Rust通过真实官方etcd拒绝同输入；原metadata与独立
  TSO迁移状态在双方同WAL重启后仍相同。Go/Rust使用不同SDK，底层错误文本不同且均原样保留：
  Go报告其发送上限，Rust当前canonical MetaStore返回Backend；未把它们冒充相同wire消息。
  RPC错误映射和完整宿主装配属于后续拥有任务，本项领域接口不接入proto。
- fmt、workspace check通过；sa_002动态metadata守卫1项/5负向控制通过，78包696生产源码。
  完整workspace test按外层任务24统一执行，零doc-tests不计行为证据。
- 各Cargo阶段动态领取槽位1，绝对CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  fixture/子进程退出后释放自有锁，保留缓存。

Go服务、SDK、官方etcd来源沿用任务001，固定kvproto f8ad079c8e9812ed5900b0c12b5f1d97171464c4；
server SHA256 364477a06e13afb29626ba57dbbd0be8245d0e9d68ad30f0dbeeda3dce3a3147。
1101项Go/模块首尾SHA和存在性一致，没有Go、模块、failpoint或plan.md修改；继续39生命周期。
GC RPC验收尚未由本项替代。

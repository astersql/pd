# 任务033：时间戳服务成员分配

状态：已完成，2026-11-05，基准e30cb150b。
[task-033-evidence.json.gz](task-033-evidence.json.gz)保存本轮全部命令、失败/通过日志与输入SHA。

canonical manager接受宿主共享RoundRobin，不另建轮询算法；地址等价复用typeutil，
副本优先级复用mcs-constant。get_nodes_count按base URL去重，registered_node返回注册拼写。
alloc_nodes读取当前持久组，检查split/merge角色，保留Go显式existing集合和自动调和两条路径：
显式请求超过唯一节点数时错误，自动路径可保留已覆盖全部可用节点的现有成员；
选择阶段10ms间隔、1s预算、跳过错过的tick，等价地址不重复分配。
取消在提交前放弃缓冲工作，在已派发提交时返回Unavailable表示结果不确定。
成功后才发布缓存；无需改写的健康组仍刷新最新元数据索引，实际revision不变。

set_nodes保留传入顺序、拼写和重复项并重置优先级，匹配Go领域方法；其注册候选校验
由HTTP调用方先使用registered_node完成，不在领域层发明Go没有的过滤规则。
set_priority更新所有等价拼写，未知成员错误时不写组。角色保护及读改写事务共用内部路径。
真实监听/任期工作者由34和131接通，33没有宣称旧任期fencing或Rust HTTP已装配。

本轮验证：

- 初始可运行red 1项，断言只保留1副本而不是2失败，退出101；补实现后通过。
- 最终键空间包43项通过/0失败/0忽略，含本项5个真实etcd用例：分配/设置/优先级、
  重复协议地址/健康无写入、外部元数据更新后的索引刷新、过渡角色/未知节点、
  实际请求大小上限拒绝提交、停服失败及同WAL重启恢复，另覆盖预先取消的调用。
- 同源只读Go测试二进制实际运行TestAutoSplitPatrolReconcilesLoadedGroups及
  TestReconcileKeyspaceGroupsUsesIndexAndRevalidates，2例通过。
  Rust固定回放Go的组1、键空间42、优先级0、http/https tso-1及tso-2候选；
  审查对齐该优先级输入后，受影响单例再次通过。其他非零优先级输入作为额外边界保留。
- mcs_033_registered_nodes_go_differential实际--ignored 1通过/0忽略：
  准入Go API模式的实际etcd注册两条服务目录，TCP监听器持有真实地址；Go注册watch发布，
  Rust宿主fixture用官方SDK读取同一注册目录，注入canonical balancer。
  Go HTTP和Rust领域以相同地址设置成员、用https等价拼写设优先级-7，完整DTO相等。
  未注册候选Go HTTP返回400，Rust注册查询返回None，双方组均保持先前值。
  该场景没有冒称实现或调用完整TSO服务RPC，也未以固定mock服务响应代替发现。
- 首次优先级HTTP路径未转义斜杠返回307；按现有Go调用约定对地址PathEscape后通过。
  复用启动helper同时回归32的在线默认组合并1例通过。
- fmt与workspace check通过；sa_002动态metadata守卫1通过，78包690生产源码、5负向控制。
  完整workspace test仍依外层任务24统一回归。环境测试默认ignore，只计实际--ignored回执。
- 每阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  所有测试、Go、监听及etcd进程退出后释放自有锁，保留缓存。

Go/模块1101项首尾SHA、存在性一致，未新增Go源码、模块差异或failpoint注入。
所有plan.md和来源史诗保持只读。

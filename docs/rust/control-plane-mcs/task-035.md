# 任务035：超大分组自动拆分

状态：已完成，2026-11-05，基准190f09b1c。
[task-035-evidence.json.gz](task-035-evidence.json.gz)保存原始阶段、Go回执、metadata及输入摘要。

patrol_auto_split加载真实持久目录，按键空间数降序、同大小ID升序选择；只处理超过40000、
至少两副本且不在split/merge状态的分组，每轮最多尝试一项。目标取[1,4096)最小未用ID，
切出原列表尾半段、保留顺序，过滤保护键空间；无可移动项继续检查后续组。
复用31的拆分算法和事务，增加可选取消/任期提交边界；新旧公开拆分API共用同一实现。
角色路径经现有Leadership事务保护，旧epoch即使同leader值也无法写入或发布缓存。
库只提供单轮API，宿主负责周期调度。

像Go的defer一样，成功加载目录后无论拆分、未拆分或拆分失败，都使用加载的成员快照
选择需要调和的组，随后再从存储重验；与34共享候选执行/错误作用域代码，不复制分配逻辑。
GroupPatrolReport保留拆分和调和结果供宿主记录，加载错误向上传递。
ID耗尽阻止拆分，但不取消Go原有的成员调和；预先取消在读取前返回不变状态。
已派发事务取消的不可确定结果沿用34约定，不伪称回滚。

本轮验证：

- 可运行red 1项：40001键空间未产生目标组，退出101；补巡检后通过。
- 聚焦8项通过，含7项真实etcd及1项纯ID边界测试；完整键空间包59通过/0失败/0忽略。
  回放Go生产阈值40000/40001、精确20000/20001列表顺序、较大组优先、同大小ID优先，
  跳过过渡/副本不足组、保护键空间构成的空尾部、最小可用ID及预先取消。
- 真正持久化Go同样的4096个分组（原生fixture按100写一批），确认耗尽后没有拆分，
  整个真实存储revision不变。取消场景实际停止存储仍正常返回不变结果，重启WAL值相同。
- 超大成员信息使实际etcd拆分请求被大小上限拒绝，源组/缓存保持原状、目标不存在；
  实际停服使巡检加载失败，同WAL恢复原状态，修复fixture输入大小后同一存储成功拆分。
- 加载快照的deferred成员调和真实执行；相同leader值新lease后旧任期巡检被NotLeader
  拒绝且目录不变，新任期巡检提交成功，取消的旧任期保持不变。
- 只读同源Go测试二进制的9个TestDoPatrolKeyspaceGroupSizeForAutoSplit系列用例通过；
  TestFindNextAvailableKeyspaceGroupID五个叶例通过（父测试pass不重复计数）。
  原Go巡检使用MemoryKV；Rust逐项回放固定输入并补真实etcd故障/WAL/任期证据，
  未把MemoryKV当成耐久性，也未启用autoSplitKeyspaceGroupThreshold failpoint降低阈值。
- fmt和workspace check通过；sa_002动态metadata守卫1通过、5负向控制、78包692生产源码。
  完整workspace test仍按外层任务24统一回归，零doc-tests不计行为证据。
- 每阶段动态领取槽位1，CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1；
  所有测试和fixture进程退出后释放自有锁，保留缓存。

1101项Go/模块首尾SHA和存在性一致，未新增Go源码、模块修改或failpoint注入。
所有plan.md和来源史诗保持只读。下一领域拥有任务为36，GC RPC验收未被本项替代。

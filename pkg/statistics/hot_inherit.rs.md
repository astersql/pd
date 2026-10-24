# 热点副本继承与主节点变化

扩展同一[HotPeerCache](hot_peer_update.rs.md)，不引入调度依赖。
`find_old_hot_peer_for_inheritance`先按缓存store ID升序查找，再按region新副本顺序去重查找。
优先allow_inherited；无候选时保留最终direct查找（包括None）。Go旧store map无固定顺序，
Rust稳定选择升序首个候选，不承诺多个不同历史候选间随机选择完全一致。

`collect_expired_items`返回副本集合之外的Remove快照，不修改缓存。
宿主须先检查flow，再依次应用expired和flow动作，使刚移除副本仍可提供历史。
所有检查均不改变缓存窗口；应用动作才推进状态与TopN。

继承使用DimStat::clone保留真实TimeMedian/累计秒并重置pending delta；直接报告复制pending delta。
继承未满窗口时allow_inherited=false，满窗冷热更新恢复资格。
历史store集合中存在但缓存已移除的cold peer，满窗热则重置1/defaultAntiCount，冷则Remove。

`check_peer_flow_at`使用宿主纳秒时钟。旧region leader与新leader不同则所有报告peer记录迁移时间，
write跳过首次变化报告（保留窗口、degree、anti），read照常累积；后续保留冷却时间。
`check_peer_flow_dynamic`传递同一now同时驱动阈值与冷却；无时钟旧接口采用固定原点0。
时间边界复用HotPeerStat::needs_cooldown_transfer_leader的严格小于比较。

独立测试在lib.rs显式注册，覆盖替换、多store、克隆隔离、pending delta、冷peer重入、
read/write差别、非leader副本变化与丢弃动作。同步内存策略不持有后台任务或MetaStore。
[证据](../../docs/rust/state/task-21.md)。

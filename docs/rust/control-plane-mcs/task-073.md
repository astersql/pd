# 任务073：隔离分组时间戳领导权

2026-11-06，基准`213abc35e`。状态：已完成。
实现与原始回执见[任务76](task-076.md)及[验收10](../server-api/acceptance/task-10.md)。

每组复用原Leadership的lease/value/revision fencing和Oracle窗口比较事务；
Oracle::new_for_group只按canonical keypath选择分组持久键，classic入口仍使用0组路径。
宿主提供participant bytes和Tokio handle，allocator拥有可取消并join的竞选、续租及时间戳更新工作。
API模式宿主不再建立classic Oracle，避免与独立默认分组争用同一窗口。

`mcs_073_`真实etcd1通过：两个实际候选竞争同一组，只有leader发号，关闭首任后新任时间戳严格增加；
实际撤销当前election lease后缓存窗口拒绝发号，重建allocator并从etcd同WAL恢复后仍严格增加。
原始时间戳、窗口与lease断言及回执均保留。Go实际TSO请求叶例通过；Rust另补跨任期与WAL故障证据。
TSO包级17项通过；classic TSO真实Go/Rust差分和受控时钟/退压/fencing/恢复各1项通过，
metadata、fmt、workspace check、架构守卫及相关宿主回归通过。
拆分/合并的跨组时间戳交接仍由74/75实现，完整workspace test集中验收24。
最终动态槽位1及早期槽位2、绝对target路径、命令/退出码及Go1101项首尾一致证据见共同回执。

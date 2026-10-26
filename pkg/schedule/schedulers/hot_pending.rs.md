# Hot pending 生命周期

`hot_pending.rs` 扩展 canonical `HotLoadProjection`，不复制任务69/86的
`PendingInfluence::weight`、operator状态转换和 `summary_pending_influence` GC。

- `try_add_pending_influence` 以真实 operator 的 region ID 去重；独占可变借用下
  单条记录检查/插入不可分割。重复不会覆盖 operator、来源或影响量。
- `collect_hot_pending_influence` 从当前方向和同region/store反向 cache peer 收集
  `get_loads()`，保留八维读写byte/key/query/CPU及一个hot peer count。
  反向cache缺失时相应方向为零；陈旧身份及不足四维的快照返回类型化错误。
- `admit_hot_solution` 使用真实求解结果，检查源/目标 TiFlash Write、Compute
  引擎是否完全一致，复用 `HotSourceSelector::max_zombie_duration` 选择窗口。
  传入同一轮有效配置（含priority兼容降级），反向peer由宿主cache查找闭包提供。
  按Go先登记主操作，再登记反向操作；反向冲突保留主记录，与来源atomicity TODO一致。
  领域API不会将此行为改成整对事务。
- `HotDispatchResult::admit_pending` 按94的有序offers逐项尝试并返回首个成功的
  真实操作器方案和pending拒绝错误。调用者在消费dispatch前保留其diagnostic和
  builder errors。提交operator、指标发布和调度snapshot装配由宿主负责。
- `admit_hot_split` 接受93的纯SPLIT真实操作器及真实region，将全部副本作为来源，
  目的store为0。持续时间由同轮来源统计策略传入。split不做源/目标引擎比较。

成功操作在整个zombie窗口继续计入影响；取消/超时/过期/被替换立即为零，窗口到期
再移除记录。GC按结束状态的reach time计算而非创建时间。每轮宿主传入新的store
summary，重复心跳不会累加上一轮summary；原有API要求仍适用。没有外部持久化，
无需新增MetaStore或mock。signed窗口<=0转换为零保持首次终态summary立即GC。

只读来源：hot_region.go tryAddPendingInfluence、summaryPendingInfluence、
balanceHotReadRegions；hot_region_solver.go tryAddPendingInfluence、
collectPendingInfluence、calcMaxZombieDur。Go运行oracle缺失，按用户授权采用
源码/测试推导，绝不声称运行差分。详见
[证据](../../../docs/rust/schedule-execution/task-095.md)、
[冻结场景](../../../docs/rust/schedule-execution/fixtures/task_095.json)。
验证覆盖真实操作器终态、22轮summary、8维采集、重复准入、读fallback、反向冲突、
split全副本影响、6类zombie来源和类型化错误；独立测试显式注册于lib.rs。

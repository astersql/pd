# 慢趋势候选选择

`slow_trend_candidate.rs` 是 `astersql-pd-schedule-schedulers` 的候选领域实现，
复用真实 `BasicCluster`、`StoreInfo` 和 `SlowTrend`，不引入协议/运行时/存储依赖。

- `choose_slow_trend_candidate` 对应 Go chooseEvictCandidate：至少3个store、唯一
  candidate、严格epsilon双趋势或raft-v2近期历史、受影响数量及慢于其他节点确认。
- `slow_trend_stores_updated` 对应 checkStoresAreUpdated：候选自身和非服务节点
  直接计数，其他有效节点要求heartbeat >= 捕获时刻，阈值ceil(n/2)。
- `slow_trend_slower_than_others` / `slow_trend_faster_than_others` 保留Go分母与
  严格epsilon、正值和10%容差；没有数据时false，不用默认统计虚构slow trend。
- `SlowTrendCandidates` / `SlowTrendCandidate` 对应capture/pop/markRecovered及
  全秒时长，当前候选与历史分开；pop(true)即便为空也覆盖历史。

时间由宿主提供Unix纳秒，capture接收两个独立读数（对应Go的两次time.Now），
要求不回退的时钟；负向间隔防御性按0处理，不声称复刻Go负浮点转uint64的
平台行为。120秒重新检查按整数秒截断，120.999999999仍在窗口内。
配置ratio直接传入；上层负责配置校验。整数阈值保留截断，不改成ceil。
None/false是Go正常无候选/数据不足路径，不制造持久化错误；该层没有I/O，
因此故障验证针对缺失统计、NaN、生命周期及多候选歧义。持久化、核心驱逐引用、
真实TransferLeader和HTTP配置由后续行为实现，不在此处创建另一套调度器。

依据是只读 `evict_slow_trend.go` 与 `evict_slow_trend_test.go` 的源码/测试推导，
已按用户授权替代缺失运行oracle；**没有Go运行差分**。测试期望手工冻结在
[fixture](../../../docs/rust/schedule-execution/fixtures/task_083.json)，详细测试、
错误谓词红灯和workspace证据见[task-083](../../../docs/rust/schedule-execution/task-083.md)。

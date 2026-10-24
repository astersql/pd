# 同步响应应用

`ClientApply::apply` 接收适配层已转换的 `SyncApplyResponse`，独占借用真实
`BasicCluster` 与 canonical history，并通过宿主提供的 `Storage` / `ClusterPaths`
完成持久化。不依赖协议类型，不创建后台任务、runtime 或全局状态。

- 任意 error（包含 code=0）只清除 running；不改变 full_syncing、索引、cache 或 storage。
- stats、buckets 仅在长度等于 regions 时整体使用；长度不等时全部丢弃。
  stats 保留四项原始计数，不做 heartbeat 过滤/单位转换。bucket 缺项可为 None。
  leaders 按位置独立读取，None、越界或 ID=0 均无 leader，不校验成员归属。
- 全量起始帧为未运行且非全量状态的 start=0 非空帧；中间非空帧的索引为位置偏移，
  不重置 history，不记录 history。空完成帧按其索引重置并标记运行。
  `reset_stream` 只重置流状态，连接管理与加载生命周期由后续客户端任务负责。
- 每个 region 经 core precheck 后跳过 stale，否则复用 region_guide 决定保存，
  真实 PutRegion 先更新 cache，再尝试保存；保存失败仅抑制本项 history。
  所有 overlaps 都尝试删除，删除失败不阻止后续区域，诊断计数返回宿主。
- `ApplyHistory` 仅桥接已有 HistoryBuffer / PersistentHistory，不复制历史算法；
  后者保留每100条检查点与reset倒计时。检查点失败保留已分配记录并计数，
  typed HistoryError 返回前仍删除 overlaps。无 MetaStore revision/history index 混用。

测试在 `client_apply_test.rs` 显式登记，真实 MemoryStore/EtcdMetaStore 覆盖缓存和
存储最终状态；故障只在存储事务边界注入。Go运行oracle、红灯及验证见
[任务37证据](../../docs/rust/state/task-37.md)。加载的 Go failpoint 闭包运行证据
仅确认现有基准，不表示本任务已移植加载/网络/取消生命周期。

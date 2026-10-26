# SplitRetry：拆分重试与取消

`SplitRetry` 是 canonical splitter crate 的宿主时钟状态机，复用
`group_keys_by_region` → `SplitAttempt::submit` → `SplitAttempt::scan`。
不复制 core、replication、factory、controller 或结果扫描判定。

宿主以单调毫秒时钟驱动：`Ready` 时 `start_attempt`，将返回的
`SplitSubmissions.admitted` 接入既有 dispatch，保留 rejected 中的类型化错误；
`Watching` 时调用 `advance`，每100ms观察当前 `RegionsInfo`，每轮60s截止；
`Backoff` 到期后继续驱动，`Done` 读取 `result`。
本对象不创建任务、timer、runtime，不持有 storage 或后台资源；drop 即停止驱动。
正在进行的轮次在结束时才合并到 result，宿主无需另行取消/join内部任务。
时钟不得倒退；晚到的tick以当前快照扫描一次，不补造历史扫描。

依据只读 `region_splitter.go::SplitRegions/splitRegionsByKeys`：

- retryLimit 包含初始轮之外的重试，负值不执行；空输入返回0%。
- 每轮只重新分组未观察的 keys，保留输入顺序和重复值。已有边界不派发，
  也不会被当作本请求新完成结果，因此可能以0%结束。
- 只有扫描观察到的start-key减少剩余keys；准入成功、epoch增长不计完成。
- 返回比例为 `100 - remaining*100/original`，整数除法在减法前完成。
  三个key完成一个得到34%。返回region IDs跨轮按集合去重并排序。
- 未完成轮后等待 `min(100ms * 2^attempt_index, 60s)`，最后一轮也等待。
- Go parent context取消结束当前扫描，外层重试与time.Sleep仍执行。
  Rust保留此行为；宿主必须继续驱动退避并把取消状态传给后续轮。
  已取消上下文仍允许该轮提交，取消本身不移除controller中的operator。
- 空/全部被拒绝的组仍等第一个tick或取消，然后结束观察。
- tick和取消/截止同时就绪时，Rust选择先取消/截止，是Go select允许的一种顺序。
  不声称复现Go随机select时序；无字段被忽略以掩盖差异。

构建/准入错误通过 `SplitSubmissions.rejected` 保留原有 `SplitExecutionError`，
这类组不扫描，其keys保持可重试；timeout/cancel不是成功也不伪造错误或ID。

证据为用户授权的**Go源码/测试契约推导 + Rust行为验证**；没有Go运行输出或
Go/Rust运行差分。来源与验证详见
[task-064](../../../docs/rust/schedule-execution/task-064.md)。
独立注册的 `task_064_test.rs` 使用真实MemoryStore/StorageEndpoint构造RuleManager，
真实BasicCluster和ControllerAdmission，只控制显式时钟与heartbeat更新边界。

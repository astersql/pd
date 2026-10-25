# Controller 限流与影响查询

[实现](controller_limit.rs) 复用 ControllerAdmission 的 canonical map、计数与等待
队列，读取 BasicCluster 的真实 RegionInfo/StoreInfo，使用现有 StoreLimiter 的
v1 token bucket 和 v2 snapshot windows。没有新增状态副本、配置版本、协议、
MetaStore 或私有 runtime。

`exceed_store_limit` 汇总候选操作的 total influence，用首操作优先级逐 store/type
检查；urgent 绕过检查，零成本跳过，缺少 store/limiter 时返回 false。v1 配置使用
SharedConfigProvider 的每分钟速率除60，未变化的速率不重置桶。v2 不被速率配置
重置。预检查不消费 token；`add_limited_operators` 只有准入成功后才 Take，替换时
先 Ack 旧操作。`promote_limited_waiting` 丢弃超限批次并继续；错误仍使用
AdmissionError/CancelReason，不吞掉队列错误。Go 中 Take 返回值也不影响准入结果。

宿主实际装配限流时使用 limited 准入/提升/移除入口；原有不带预算的入口保留为
底层组合 API。Ack 同样读取当前配置刷新 v1 速率；配置不变时保留 token 债务。
移除或替换必须只对实际移除的成员 Ack，旧引用不能重复退款。
v1 Ack 不退款，v2 Ack 回收 snapshot 使用量。底层 `ack_removed_operators`
适用于宿主自行组合 AdmissionOutcome，调用者不得重复 Ack。

准入 mutex 覆盖 check/admit/take，与移除/替换互斥；store 的共享 limiter 锁
串行化配置 reset/available/take/ack。移除先按引用摘除 map 成员，释放 map 锁后
Ack；并发准入在 Ack 前可能暂时见到旧额度，与 Go 的分步移除一致。
沿用 core 的 async limiter API，跨 await
持有同步 guard，future 为 !Send；宿主需在当前任务/LocalSet 驱动，调用前不能
持有 store guard，回调和 clock 不得重入控制器。此层不安装 runtime 或后台任务。

`get_op_influence` 从在途快照计算未完成步骤，排除 timeout/success、缺失 region、
不满足任何非 nil option 的 region。CheckTimeout 保留状态变化。`with_range_option`
照 Go 直接比较原始字节，多个范围取交集，空列表接受存在 region；空 end key
也按空字节处理。没有套用 KeyRange.contains 的正无穷语义。

`FastOperatorInfluence` 是 dispatch 宿主的短期成功缓存，以 region ID 替换，插入后
10秒过期；显式 monotonic Instant 在查询时清理，无后台线程。宿主负责判断成功且
运行时长小于10秒；缓存查询聚合缓存的 total influence，不重新过滤成未完成步骤。
缓存 expiration == now 不计入，与 Go GetAllID 的严格大于比较一致。

Go 来源为 operator_controller.go 的 ExceedStoreLimit/addOperatorInner/ack、
GetOpInfluence/GetFastOpInfluence/NewTotalOpInfluence/WithRangeOption，及对应
TestStoreLimit/TestInfluenceOpt/TestCalcInfluence/TestCacheInfluence 和并发重设测试。
本次遵循用户授权，期望来自只读源码与测试契约，未执行 Go，未声明运行差分。
行为测试在 [task_035_test.rs](task_035_test.rs)，来源哈希、失败/通过证据与实际
验证命令见 [task-035.md](../../../docs/rust/schedule-execution/task-035.md)。

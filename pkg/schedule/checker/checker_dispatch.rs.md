# CheckerDispatcher：检查器分派与待处理集合

生产入口为同目录 `checker_dispatch.rs`，由 canonical
`astersql-pd-schedule-checker` crate 导出，无新 crate 或生产依赖。

`check_region` 使用宿主同一 `BasicCluster`、配置、`RuleManager`、affinity
`Manager`、`RegionLabeler` 和 `ControllerAdmission`；仅拥有 checker 与队列状态。
`CheckerDispatchContext.affinity.rules.replica.stores` 应指向 `cluster.stores`，时间为
宿主 Unix 纳秒快照；邻居从 canonical region tree 查询。无 protobuf、私有 runtime、
全局状态或集群副本。ID 分配是唯一可失败的宿主输入，类型化诊断随结果返回。

## 分派

只读 Go `checker_controller.go::CheckRegion` 的顺序为：

1. joint-state；
2. split；
3. placement 开启时使用 rule，否则 learner → replica；
4. `schedule=deny` 标签守卫；
5. affinity；
6. merge。

首个非空结果立即返回，双 merge 操作保持完整。暂停按七个 Go 名称路由，
未知名称返回 None。rule 的单一暂停控制所有组成阶段；replica 同理。
rule 先查真实 fit-cache；未命中先执行 PriorityInspector，再检查 replica 限额。
replica 模式先规划再检查限额，learner 恢复不受 replica 限额约束。
affinity 和 merge 先检查各自真实运行数量；等于限额即拒绝。

rule 使用真实 orphan → 每条 rule 的 missing 或 fault/role → location 顺序。
仅将已有 RuleFaultChecker 的 `fix_rule` 可见性改成包内，保留完整 fit 供约束校验，
不复制 per-rule 算法，也不先扫描所有 missing 再扫描所有 fault。失败继续下一条 rule，
最终的 fit-cache 更新使用真实 region/store/fit 校验。

## Pending 与宿主接线

宿主创建并传入共享 `Arc<TTLUint64<()>>`，TTL 为 3 分钟、GC 间隔为 1 分钟，
使用自己的 Tokio handle 和 cancellation token，关闭时 await `shutdown()`。
复用 canonical cache，因此 Get 的到期相等边界、枚举严格未到期、
物理长度包含未回收过期项等语义保持一致。
容量守卫为 `len > 100000`，受限批量插入最多到 100001 项；
超过后连覆盖已有 ID 也拒绝；unchecked 插入和 checker 的内部 retry 插入绕过容量守卫。
rule 的永久失败集合使用 canonical 有界 2Q Cache，与临时 retry TTL 集合独立；
成功生成 rule 操作时移除永久失败集合成员。

`try_add_operators` 保留缺失 region 与 no-op 的 pending，已有运行操作时移除 pending；
实际存储预算拒绝时受限加入，交给 waiting queue 后移除，即使队列拒绝。
该函数交付 canonical waiting 队列，宿主随后调用
`promote_limited_waiting` 并处理 admitted/removed、dispatch 和 records，
与既有 operator crate 的宿主边界一致；此任务不装配 coordinator 巡检循环。

宿主向 `ControllerRecords` 注册 `success_callback`，传入 `Weak<Mutex<CheckerDispatcher>>`
及动态 affinity 限额/秒时钟函数。完成后才写入 affinity 双 region 的 60 秒合并缓存，
重复 burial 由 canonical records 按操作身份去重。宿主须在释放 dispatcher 锁后
调用 records，避免重入同一锁。非 witness 的 heartbeat 步骤完成单独调用
`record_region_promote_to_non_witness`，不混同 whole-operator success。

## 证据与限制

依据用户授权，本任务仅为 Go 只读源码/测试契约推导及 Rust 行为验证，
未执行 Go oracle，未声明 Go/Rust 运行差分。
来源、fixture、失败验证、变异验证和最终命令见
[任务 055 证据](../../../docs/rust/schedule-execution/task-055.md)。
独立测试文件 `task_055_test.rs` 显式注册，使用真实 core、placement、标签、
affinity、内存 MetaStore、operator/records、TTL 和存储令牌。

任务58增加 `set_metrics(CheckerMetrics)`，在既有分派阶段接入宿主 Registry 的
checker duration 与调用、暂停及可观察结果事件；不改变领域选择、错误或队列决策。
具体覆盖与有界事件 API 见 [指标说明](checker_metrics.rs.md)。

任务113增加只读 `rule_checker` / `merge_checker` 访问器，供 Coordinator 的锁定视图
访问 dispatch 使用的同一个组件；不创建新 checker。成功回调和 pending 接口保持原样。

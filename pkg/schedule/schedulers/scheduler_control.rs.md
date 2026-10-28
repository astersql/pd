# 调度器管理状态

`SchedulerController` 是 canonical registry 的领域管理者，持有真实
`RegisteredScheduler`，以 `&mut self` 串行处理 `add/remove/pause_or_resume/reload`。
宿主提供真实 `BasicCluster`、`FactoryContext` 和完整 cluster JSON 配置；管理者只修改
`schedule.schedulers-v2`，保留其它字段。没有 HTTP handler、私有运行时、后台调度循环
或全局状态。普通策略通过 `get` 读取；HTTP 适配层通过 `handler/handler_names` 取得
领域对象。非空 handler-only 索引优先，否则回退到普通调度器索引，符合 Go。

新增普通调度器先 prepare、再保存配置，最后发布索引；失败配对 clean。
handler-only 新增先检查持久历史，保存后 prepare；prepare 失败清理部分 cluster
副作用，只删除此次新建的配置，保留已有历史。重复检查分别作用于各自索引。
移除先修改集群配置并持久化，再设置 disabled、删除独立配置、clean 并移除索引。
与 Go 一致，前序持久化失败后不回滚已修改的集群配置；配置删除失败保留可重试的
活跃索引。默认策略保留 disable=true 条目，其它策略删除首个同类型条目；新增可重新
启用已有同参数条目。SetDisable 的保存错误及 clean 的 best-effort 错误进入
`cleanup_errors`，不覆盖主错误。存储、工厂、prepare 及策略 reload 主错误保留类型。

暂停由宿主传入 Unix 秒时钟；TTL>0 设置起止秒，TTL<=0 清零；`all` 只遍历普通索引。
严格 `now < delay_until`，在到期秒及之后 `delay` 返回 `(0,0)`。采用 wrapping_add
对应 Go int64 加法。`is_disabled` 读取同一 cluster 配置条目，与暂停状态独立。
reload 仅适用于普通索引；handler-only/缺失/未 bootstrap 返回领域错误。
策略 reload 在解析成功后发布配置，失败保留 policy 配置及暂停计时器。
evict/grant 等复用已有 pause reconciliation，重复 reload 不增加 store 引用。
BalanceRegion/Label/RandomMerge/ShuffleLeader/TransferWitness 继承 Go Base 的空 reload。
hot 使用既有四资源快照构造边界重建 solver 配置（快照轮次内的搜索状态随重建重置，
与既有 HotScheduler 宿主快照契约一致），pending 仍由外部 canonical pending 管理者持有。
slow-store 的 disk/network 组件共用管理索引与持久配置，重载 network 恢复计时保持不变。

宿主须保持这些命令 future 执行完成，并在自己的生命周期边界串行管理 loop；本任务
不提供 spawn/stop/join，后续调度循环任务负责这些能力。管理器不代替每个具体策略的
operator 限额、halt、placement 或 readiness 判断。

依据：只读 `scheduler_controller.go`、`server/config/persist_options.go` 以及各策略
ReloadConfig/PrepareConfig/CleanConfig。按用户授权采用源码推导，不声称 Go 运行差分。
验证与冻结来源哈希见 [task-107](../../../docs/rust/schedule-execution/task-107.md)。

任务114增加 `replace_scheduler_configs`：启动恢复最终发布已验证的完整列表，保留
重复及 disabled 条目，删除无法构造或添加的旧条目。它只替换同一管理器的
`schedule.schedulers-v2`，由恢复入口调用既有 StorageEndpoint 保存完整配置。

`clean_registered` releases prepared scheduler and handler store controls after
coordinator workers join. It retains the management entries and persisted config;
cleanup failures remain in `cleanup_errors`. Task 115 validates repeated stop and
actual store-state restoration without deleting scheduler configuration.

`get_mut` permits a host with exclusive management access to drive the restored
policy itself. The runtime serializes execution and management, and runs
`clean_registered` only after its worker joins. It does not create a replacement
policy just to access a concrete scheduling algorithm.

Before a serialized restoration/persist, `set_configuration` refreshes the complete
host document under the same admission gate as other configuration workers. It
prevents a startup snapshot from overwriting StoreConfig committed while the
preparation gate waited. It does not recreate registered policy instances.

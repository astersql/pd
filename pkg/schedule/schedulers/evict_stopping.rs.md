# 停止节点驱逐恢复

`EvictStoppingConfig` / `EvictStoppingScheduler` 复用 canonical BasicCluster 的停止驱逐引用、
StorageEndpoint/MetaStore，以及 EvictLeaderScheduler 的真实操作器、过滤器和规则。
领域 API 为 `observe`、`schedule`、`prepare`、`clean`、`reload`、`update`；
HTTP/指标适配不进入本模块，无新依赖、运行时、feature 或独立状态副本。
宿主串行调用配置与调度方法；Prepare 在装配时调用一次。

依据 `evict_stopping_store.go` 与相邻 suite 只读推导（用户明确授权替代运行 oracle）。
仅唯一 Preparing/Serving 且 IsStopping 的节点触发，多个节点则不触发；已选节点不再
要求 Serving，Removing 但仍 stopping 时继续驱逐。缺失/Removed/恢复即清理，
同一轮不捕获替代节点。leader 限额仅在已经有 evicted-store 时检查。
默认 batch=3、reload batch=0恢复默认；配置 update 接受有限数值 [1,10] 并按 Go 截断。
加载配置与共享 evict helper 一样接受 batch 1..100；非法类型/ID由 serde 拒绝。
JSON 保留 evicted-stores 数组（包括历史多元素配置），Prepare/Clean 仅操作首个ID，
调度对整个数组生成全键范围驱逐，与 Go 一致；Reload 不调整集群引用。

写入成功后才发布内存快照；捕获 I/O 失败/取消释放新取得的引用。
Clean 写失败仍恢复引用而配置回滚，重复 Clean 可继续降低 Go 的有符号引用计数，
这是来源行为。取消 clean 时快照尚未发布、引用尚未释放；宿主可重试。
类型化 EvictStoppingError 传播配置、存储、store control 错误。
调度保留配置清理失败时的旧 snapshot 并仍尝试 leader 驱逐，错误单独返回。

验证见 [task-082](../../../docs/rust/schedule-execution/task-082.md)，
测试独立于实现并显式注册；固定期望来自 Go 分支，不声称取得 Go 运行差分。

任务142新增 `update_http(Option<f64>, disabled, storage, paths)`，仅在 batch 存在时
校验 [1,10]，缺省保存历史 batch；写入宿主 disabled envelope 后才发布配置。
宿主 disabled 的生命周期由装配层管理，不由请求或本领域对象重新持有。
验收见 [task-142](../../../docs/rust/schedule-execution/task-142.md)。

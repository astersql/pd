# ReplicaStrategy

对应只读 Go `replica_strategy.go`，在 canonical checker crate 中复用真实
core StoreInfo/RegionInfo、filter StoreCandidates/StoreStateFilter 和 placement
RegionFit/RuleFit。借用宿主的一代配置、store 快照和显式 Unix 纳秒时钟；异步
预算检查使用宿主 runtime，不创建私有 runtime 或新状态副本。

`select_store_to_add` 先用排除、容量、special-use、允许临时状态的 store-state、
隔离和两组 extra filters 筛选并保留最高多样性，再严格检查临时状态、选择最低
region score。无永久候选返回 `(0,false)`；最佳位置全部暂不可用返回 `(0,true)`，
不会回退到较差位置。fast-failover 使用 URGENT、跳过目标预算/snapshot/pending
限制，仍检查 disconnected/busy。正常路径使用 HIGH。

`select_store_to_fix` 允许位置退化，单副本保留比较对象；`select_store_to_improve`
要求严格提高多样性。为保持输入快照不变，Go 的 swap 操作在本地 Arc 顺序副本上执行，
包括旧 store 不在列表时依然去掉第一项的源行为。候选 store 本身不复制。

`select_store_to_remove` 先严格状态过滤再比较最差位置和最高负载；带 temp-state
版本先固定最差位置，在该集合中优先健康源，否则返回暂不可用源和 true。
移除允许 removed/down source；不能将目标的 Removed/Down 谓词加到源过滤。
`get_better_location` 用规则 peer 查 store，忽略缺失 store；比较集合限定相同 engine、
匹配相同角色规则之一的 region stores。暂不可用源若存在改进可能，只返回等待标志。
使用 placement 已有的 `RegionFit::rules()`，未扩展 placement API。

`is_witness_enabled` 同时检查配置许可和版本兼容。宿主通过 major/minor 投影保持
其版本类型独立于 checker；Go 的 `IsCompatible(6.6.0, version)` 对 6.6 任意 patch/
prerelease 都接受，故不引入严格 patch 比较，也不依赖 server/config。

Go 策略的无解和临时拒绝用 ID/布尔结果表达，未构造新的错误类型；该单元没有
持久化、网络或 MetaStore 调用点。随机不参与选择；平分使用已有候选遍历顺序。

[task-040 证据](../../../docs/rust/schedule-execution/task-040.md) 记录源码 provenance、
测试输入修正、真实行为回归注入、完整检查和共享 Cargo 槽位。Go 运行要求已由用户解除，
所有期望为只读源码/测试推导，未宣称 Go 执行或运行差分。

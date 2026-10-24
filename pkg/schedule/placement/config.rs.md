# config.rs

`RuleConfig` 使用私有 BTreeMap 和不可变 Arc<Rule> 保存规则；`rules/groups/get_rule/get_group`
读取快照，`set_rule/set_group` 构造配置，`adjust` 归一默认组、设置运行组元数据并重算引擎摘要。
`may_restrict_store_load` 分别读取 TiKV/TiFlash 缓存；`begin_patch` 保存独立基线。

`RuleConfigPatch` 的 `set_rule/delete_rule/set_group/delete_group` 记录覆盖与删除标记；
`get_group` 优先补丁、其次基线、最后默认组。`adjust` 返回有效规则预览，保留显式默认组，
重新计算预览引擎摘要，不改动旧规则 Arc。`trim` 按序列化 JSON 比较，忽略解码 key 和运行 group，
保留 version/create_timestamp 的区别；缺失规则的重复删除和缺失组的默认重设被裁掉。
`mutation_counts` 提供裁剪后的计数。`commit(now_unix_seconds)` 消费补丁、返回新配置，
旧规则 version 包装加一且保留创建时间，新规则版本为零并采用调用者时钟，再归一组与缓存。
调用方负责先 adjust/验证、trim、存储成功后发布 commit 的结果；这里不实现存储事务或并发发布。

`rule_may_restrict_engine` 复用 canonical LabelConstraint 的匹配语义，以空标签和 tikv
代表 TiKV、tiflash 代表 TiFlash；非 engine 约束或同一引擎部分表示匹配才限制该引擎。
空约束由配置聚合层跳过，多个 engine 约束取交集，不将 engine-only TiFlash 误判为限载。

没有协议类型、MetaStore、I/O、全局状态、后台任务、私有 runtime 或系统时钟。
生产实现对照 config.go，独立测试显式登记于 lib.rs；见
[批次 23 证据](../../../docs/rust/schedule/placement/23-证据.md)。

批次 25 的 crate 内只读 `rule_mutations/group_mutations` 供 manager 持久化 trim 后的补丁，
不把存储写入领域配置，也不公开内部 map 给调用方修改。

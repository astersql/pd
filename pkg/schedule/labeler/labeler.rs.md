# RegionLabeler 存储与恢复

Canonical crate 为 `astersql-pd-schedule-labeler`，[lib.rs](lib.rs) 导出
[实现](labeler.rs) 的 `RegionLabeler` 与 `LabelerError`。
Go 契约为 [labeler.go](labeler.go) 的 loadRules、SetLabelRule、
SetLabelRuleLocked、DeleteLabelRule、DeleteLabelRuleLocked。

- `RegionLabeler::load(StorageEndpoint, ClusterPaths, Timestamp)` 扫描真实 endpoint，
  校验并按内嵌规则 ID 收集，后扫描的重复 ID 覆盖前者。读取全部成功后，按物理键
  顺序逐条事务删除无效 JSON/内容/完全过期规则，最后构建完整 LabelRuleIndex。
  部分标签过期仅在内存过滤，不重写原持久值；键后缀与 ID 可以不同。
- `index()` 只读借用权威规则及通用/键空间索引，不提供可变派生视图。
- `set_label_rule(rule, now)` 先校验、事务持久化，再发布内存与派生索引。
  `set_validated_label_rule(rule)` 对应 watcher 已校验入口，调用方必须先
  check_and_adjust/from_json；不会重复校验或改变 TTL 起点。
- `delete_label_rule(id)` 先持久删除，内存不存在则返回现有 PD catalog 的
  ErrRegionRuleNotFound。`delete_label_rule_if_present(id)` 对应 watcher 的幂等删除。
- `LabelerError::{Rule,Load,Transaction,NotFound}` 保留验证、加载、事务以及缺失规则
  的不同错误来源；事务错误保留 callback/store/conflict 分类，不静默重试。

所有修改需要 `&mut self`，宿主在共享使用时提供同步；异步 I/O 中不持有内部同步锁。
失败写入不发布内存状态。事务提交后立即更新索引且没有中间 await。
构造失败不暴露不完整 owner；清理事务先前已成功的删除会保留，符合 Go 行为。
后端在传输错误下可能已提交，沿用 MetaStore 的未知提交结果语义；此时调用者应重载。

时间由宿主明确传入，单位为 Unix 纳秒。本批只实现存储壳，不启动 GC/runtime，
后续批次负责计划/批量修改、区域标签解析和 GC；原始 index 不替代过滤后的查询 API。
存储复用 canonical endpoint/kv/keypath；没有新存储实现。领域规则和公开签名没有
proto 类型，endpoint 的已有 protobuf 适配为传递依赖，详情见
[批次 17 证据](../../../docs/rust/schedule/labeler/17-证据.md)。

独立测试 [label_restore_test.rs](label_restore_test.rs) 由 lib.rs 显式登记；真实内存
和 EtcdMetaStore 使用相同契约。I/O 故障仅在 MetaStore read/transact 派发前注入，
其余调用完整委托真实后端，不替换范围计算或成功事务结果。

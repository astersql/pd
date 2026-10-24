# rule_batch.rs

`RuleManager::begin_patch/try_commit_patch` 实现 owned 补丁的验证、JSON trim、持久化与发布。
宿主必须序列化 begin/commit 与其它变更，不能跨并发写入持有旧补丁。
真实 RuleList 验证在写入之前；规则操作先于组操作，每 120 项一个 endpoint 事务。
失败不发布内存，但之前成功的事务留在后端，不能将整个批量操作视为原子事务。
同类操作使用 BTreeMap 顺序；Go map 顺序未定义，因此中途失败只保证分批数量和类别边界，
不保证 Go 恰好保存相同的规则子集。保存 incoming version/time，再由 commit 更新内存版本。

`set_rules` 校验所有规则再提交，重复逻辑键最后一项胜出；`batch(Vec<RuleOp>)`
先校验所有 add，按顺序处理 add/del，未知 action 忽略。前缀删除只枚举原始配置；
同一批次刚添加且原始不存在的规则不会被前缀删除选中。
`set_rule_group/delete_rule_group` 设置或重置组配置，不删除组中的规则。
`set_group_bundle` 替换一个组；`set_all_group_bundles` 根据 override 替换全部或指定组；
组 ID 自动填入缺省 rule.group_id，显式不一致拒绝；空规则集合仍需通过覆盖验证。
`delete_group_bundle` 删除匹配组及规则，regex=false 精确匹配，regex=true 使用正则非锚定查找。
正则适配显式保留 Go ASCII Perl 字符类与 word boundary；非法正则在产生补丁前返回错误。

没有协议类型、全局状态、后台工作、系统时钟或私有 runtime。存储复用 canonical
StorageEndpoint 和 MetaStore。独立测试 placement_batch_test.rs 显式登记在 lib.rs，
内存和 Etcd 使用相同契约；见[批次 25 证据](../../../docs/rust/schedule/placement/25-证据.md)。

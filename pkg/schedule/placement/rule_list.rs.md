# rule_list.rs

批次 22 的协议无关规则排序与覆盖列表，复用 canonical rangelist::Builder/List。

- `compare_rule`：按 group index、group ID、rule index、rule ID 的元组排序。
- `prepare_rules_for_apply`：输入已排序规则；同组最后一次 rule override 截断此前规则，group override 截断此前组，后续组继续保留。
- `check_apply_rules`：leader 总数不可超过一，leader + voter 至少一；follower/learner 不满足 leader 要求。计数保持 Go int64 的环绕行为。
- `BuildRuleListError`：保留 `PD:placement:ErrBuildRuleList`、Go 消息前缀和大写 hex 范围；实现 Error/Display。
- `RuleList::build`：输入 `Arc<Rule>`，范围须已解码；为每个范围验证选中规则。保留原始规则身份和 runtime group，避免 Rule.Clone 的 group 清除语义。
- `get_rules_by_key`：返回所有排序规则；`get_rules_for_apply_range`：只返回单一完整范围的选中规则，跨界返回空切片；end 为空表示正无穷，end 等于断点可接受。

与 Go 一致：第一个 start 非空时不额外补缺失前缀；但实际生成的空段（内部缺口、末尾）报错。
没有 I/O、MetaStore、副作用、时钟或后台任务。类型不依赖 proto/transport。
独立测试显式注册于 lib.rs；固定 Go 运行输出、红绿测试及依赖树见
[22-证据](../../../docs/rust/schedule/placement/22-证据.md)。

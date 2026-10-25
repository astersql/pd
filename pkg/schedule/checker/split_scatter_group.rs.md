# 拆分散射分组键

`resolve_split_scatter_range_hint` 使用真实 `RegionInfo` 的 memcomparable 起止键，
返回 `SplitScatterRangeHint { start_key, end_key, scatter_group }`。复用 canonical
codec 的字节、整数和表/索引前缀编解码，不依赖协议、存储或运行时。

与只读 `split_scatter_group.go` 保持一致：

- 事务 keyspace 只有经调用者的 `Option<&dyn Fn(u32) -> bool>` 验证后才剥离。
  调用者负责确认事务范围的左右边界；None/false、raw keyspace、不完整前缀不剥离。
  这里不实现后续 split-scatter controller，也不把所有合法 ID 自动视为已知范围。
- 表 ID 后不是完整 `_i` 前缀时按表分组；完整 `_i` 后缺少整数则返回空 hint。
  ID 保留完整 i64 范围，包括负数；分组字符串与 Go 的十进制格式一致。
- 索引的有限 end 不大于索引排他上界时保留索引组，相等也保留；跨实体、跨表、
  跨 keyspace 或无穷上界退回起始表组。记录键直接使用起始表组。
- 起点只解码第一段 memcomparable，允许未消费后缀；终点按编码字节比较，不解码，
  不添加 Go 未做的范围合法性检查。
- 前缀上界对原始实体前缀从右至左进位并截断，再拼 keyspace 并编码。
  空/全 ff 前缀返回无穷上界，不向 keyspace 进位，不改变输入。

损坏编码和不足长度沿 Go 契约返回空 hint，不产生 panic 或新的错误通道；没有
外部 I/O、时钟、持久化或需要注入的 I/O 故障。验证器是生产 API 的输入契约。

依据用户授权，本次仅以 Go 源码与已有测试契约推导期望；未执行 Go oracle，
不声称运行差分。固定输入、逐字节输出、来源哈希、失败/通过及变异验证见
[任务52证据](../../../docs/rust/schedule-execution/task-052.md)。测试独立注册于
`task_052_test.rs`，检查所有三个 hint 字段和前缀 successor，无归一化字段。

# 热点桶拆分领域决策

`hot_split.rs` 只读取 canonical `RegionInfo`、`RegionBuckets`、`BucketStat`
和已验证的 `HotRegionParams` 配置快照。`HotSplit` 复用 `HotRankV1/V2::check_loads`
实现 too-hot 门控，不复制 first/all/any 规则。桶维度不含 CPU；优先第一个受支持
维度，再第二个，再 byte；读写方向映射使用 statistics-utils。

`by_load` 只保留至少一个端点严格位于 region 内的桶，保持来源顺序和 uint64
回绕，累加至总负载整数除二，优先桶 end key，越界时回退 start key。空、单桶、
零/回绕负载安全返回空。`accLoads` 是选中桶之前的累积量，保持 Go 语义。
`by_size` 对 core 有效桶的内部 key 取上中位数。二进制 key、无穷边界复用 keyutil。

两者都使用真实 `create_split_region_operator`，保留 `OpKind::SPLIT`、USEKEY、
region epoch/range、原读写 description 和 builder 的 joint-state 错误。
这里的 rank 5 是方案优先级，不是修改 operator 为 HIGH priority。
`for_hot_region` 为 solve 的目的节点枚举前提供 load split；无结果可继续均衡。
`before_move` 为主/反向 region 提供 size split；`Some` 表示必须拆分，哪怕无
可用 key 也不能回退移动。禁用桶、leader transfer、大小恰好等于上限均不触发。

宿主负责热度选择、同轮统计快照和 scheduler 装配；本文件不拥有 bucket worker，
不提交 operator、不写 pending、不执行 I/O。完整调度装配交由任务 94。
直接拆分错误保留类型；批量 size 拆分按 region 收集 errors，继续保留其它成功
split，调用方可以记录失败并继续其它候选。

Go 来源为 hot_region_solver.go 的 bucketFirstStat、splitBucketsOperator、
splitBucketsByLoad、splitBucketBySize、createSplitOperator、buildOperators，
hot_region.go 的 tooHotNeedSplit，以及 v1/v2 的维度门控。
按照用户授权，fixture 预期由只读源码及邻测推导，未运行 Go、未声称运行差分。
来源摘要、冻结 fixture、失败与通过证据见
[task-093](../../../docs/rust/schedule-execution/task-093.md)。

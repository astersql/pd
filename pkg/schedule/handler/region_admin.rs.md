# 范围复查与复制状态

`region_admin.rs` 扩展 canonical `Handler`，提供
`accelerate_regions_schedule_in_range/in_ranges`、`adjust_limit`、
`check_regions_replicated` 和 `check_region_placement_rule`。
`pre_check_for_region` 复用 `admin_peer.rs` 的已验证实现，返回原始 RegionInfo Arc；
传输层按已有类型将未初始化、非法 ID、缺失 region 映射为 500/400/404。

范围查询使用 Coordinator 的真实 RegionsInfo。limit 对每个 range 单独生效，
非正值为全量，空 end 为无限；hex 解码复用本包已有实现，先解码再检查 bootstrap。
多范围入口先检查 bootstrap，再检查输入长度；无错误时统一入队，队列去重。
加速直接加入 checker 的 pending-processed TTL 队列，显式不受正常重试容量限制。
它与规则检查器的 pendingList 不同，不能用加速队列冒充 PENDING。

复制状态查询传入宿主持有的 RuleManager 和纳秒时间，使用当前 stores/config 的
canonical `is_region_replicated`。传统模式要求没有 learner 且副本数精确匹配；
规则模式执行真实 fit。只有未完全复制且实际存在于规则 pendingList 的 region
产生 PENDING；其它未完全复制产生 INPROGRESS；空范围或全部满足产生 REPLICATED。
规则查询返回 canonical RegionFit，并传播未初始化/规则禁用的领域错误；不创建
私有规则管理器、缓存、运行时或 protobuf/HTTP 依赖。规则持久化由现有 RuleManager
负责，handler 的这些动作没有 MetaStore 写入，测试不伪造不存在的 I/O 故障。

limit 解析采用 Go 64 位 Atoi 的十进制符号、溢出和空白拒绝行为；默认 16，仅取
第一个可选默认值，上限 10240，保留零与负数。错误为类型化 InvalidLimit，错误
展示由适配层决定。placement-disabled 文本保留 Go 原文。

Go 依据是 `handler.go:1113–1186,1274–1354` 和
`tests/server/api/region_test.go` 的范围与复制状态测试。用户授权以源码推导和真实
Rust 行为替代缺失 Go 运行 oracle；没有声称运行差分。详情及冻结 fixture 见
[任务证据](../../../docs/rust/schedule-execution/task-124.md)。

# 热点扰动配置 HTTP 适配

`serve` 接受宿主独占借用的 canonical `ShuffleHotScheduler`、`StorageEndpoint`、
`ClusterPaths` 和相对 HTTP 请求。POST `/config` 更新 limit，GET `/list` 返回缩进 JSON；
未知路径404明文，错误方法405空body，不设置Allow。宿主负责挂载、同步和HEAD线端抑制。
Go 的 shuffleHotRegionScheduler 有专属handler，不能使用BaseScheduler默认响应。
不新增完整 server/api 路由装配、crate、feature 或配置扩展点。

只读 shuffle_hot_region.go 的 updateConfig 将JSON解码为map后只接受小写limit的数值，
缺失/null/其他类型返回400 JSON字符串 `invalid limit`；未知字段忽略，重复字段取最后值。
数字先按float64表示再截断；测试包括0、小数、科学记数及2^53附近精度损失。
负数沿用相邻适配器的有符号转换再转uint64约定；负数/超界浮点到uint64的Go结果
具有平台相关性，本任务不声称取得其跨平台Go运行一致性。实现没有新增范围校验。
无归一化数值或状态差异。任意损坏JSON的全部诊断、超出float64范围的数字以及
Gorilla路径清理/重定向不在固定案例的一致性声明中。

保存通过已有领域 `update`：写成功后发布，写失败保留旧内存和持久化limit，返回500
JSON错误字符串。宿主持有独占借用，因此与Go锁内写入后失败回滚的可观察状态一致。
成功body为 `null`。请求体先全部读取，读取失败返回500/internal且不应用部分配置；
固定语法/顶层类型错误为400/input。列表/拒绝路径不读body也不访问存储。
错误栈只比较存在和结构，保存故障使用真实Rust MetaStore错误文本，不伪造Go运行输出。

按用户授权以只读Go源码/测试推导加真实Rust行为、故障验证替代缺失的Go运行oracle。
[任务149证据](../../docs/rust/schedule-execution/task-149.md)记录fixture、来源哈希、红绿测试、
实际调度、验证数量和边界。更新limit通过真实 `is_schedule_allowed` 控制调度准入；
调用方先准入再调度。无热点、placement、缺副本及分配故障复用canonical领域路径。

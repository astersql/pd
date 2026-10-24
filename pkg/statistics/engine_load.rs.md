# 分引擎负载聚合

`summary_stores_load` 复用 `summary_store_infos`、`StoreSummaryInfo`、`StoreLoad`、
`StoreLoadPred`、`StoreKindLoads`、`StoreHistoryLoads` 和真实 `HotPeerStat`。
输入为调用方持有的 store、负载与热点快照，输出为按 store ID 排序的 `BTreeMap`。
TiKV/TiFlash 分别累计人口并计算期望与标准差，compute 和缺失负载不进入人口。
TiKV leader 要求双向 transfer 允许，region 不按 transfer 或 removed 状态过滤；
TiFlash 只处理 write engine 的 region。未知 resource kind 返回空集合。

TiKV 读使用 store bytes/keys/query/CPU，写 leader 使用 leader peer bytes/keys 和
store write query，写 region 使用 store bytes/keys。TiFlash 读为零，写 region
按 trace 开关使用 store regions bytes/keys 或 peer 总和；其余维度为零。
热点筛选保留输入顺序、使用 GetLoad 去噪，输出 clone 不共享滚动状态/负载数组。
pending influence 复用既有预测代数，仅影响 future；current/expect/stddev 独立持有。

`StoreLoadSummary::add/result` 计算每维均值、历史物理槽均值（缺槽按零计）及
sqrt(variance/N)/mean；无热点人口时 stddev rates 为零，零均值维也置零。
stddev.hot_peer_count 保留期望 count，历史为空。`result` 要求传入与 add 相同人口的
current details。`StoreLoadDetail::is_uniform` 为严格小于阈值，保持负均值符号。

宿主显式传入单调 `Duration`；先读历史，再 add_store 采样，removed 不重新创建历史。
调用方负责在聚合前 gc 过期 store 和同步访问；不创建线程、运行时、指标全局或后端。
HTTP 展示与 Prometheus 指标不在本行为范围内。
测试在 `engine_load_test.rs` 独立登记；oracle 与红绿/回归证据见
[task-24](../../docs/rust/state/task-24.md)。

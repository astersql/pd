# 负载预测值代数

来源：Go `store_load.go` 的 Influence、StoreSummaryInfo、StoreLoad、StoreLoadPred、
MinLoad、MaxLoad，基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`。
Go oracle及红绿验证见[任务13证据](../../docs/rust/state/task-13.md)。

复用[历史负载](store_history.rs.md)的Loads四维byte/key/query/CPU和HistoryLoads，
复用[存储容器](stores.rs.md)的StoreKindLoads十二维；新增RegionKindLoads八维。
维度映射来自canonical statistics-utils的RWType::region_stats：读[0,1,2,6]，写[3,4,5,7]。
无新依赖、feature、协议类型或调度决策。

Influence拥有可变长Vec与hot_peer_count。StoreSummaryInfo拥有core StoreInfo快照和
Option<Influence>；summary_store_infos按ID收集，重复ID最后一个生效。
add_influence忽略None和零权重；首次按输入长度分配零向量；按权重累加全部维度及count。
后续短向量只加前缀，长向量panic，与Go切片索引契约一致，不静默截断或填零。

StoreLoad::to_load_pred借用输入，Current深拷贝历史，Future只复制率和count；
有影响时按读写选维累加，未知RW只累加count。已知方向遇到缺失region维度panic。
Expect/Stddev默认零值，留给后续汇总填充。

Min/Max逐维取极值，并独立比较hot_peer_count，历史为空；Pending为Max-Min但count恒0，
Diff包含count差。所有计算返回拥有数据的独立值，不改变输入快照。
极值辅助函数保留Go math.Min/Max对NaN、正负无穷优先级及正负零的约定。

纯同步内存代数，不持有锁、时钟、存储、网络、runtime或后台任务。
独立load_values_test显式登记，覆盖加权、快照隔离、读写/未知方向、空切片/越界、
相等极值、浮点特殊值，以及既有Go pending序列的源/目标数值。

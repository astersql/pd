# 区域汇总与过滤

对应 Go `region.go` 的 RegionStats/GetRegionStats/Observe。公开 RegionStats、
get_region_stats、RegionStatInformer、HotPeerSnapshot 与 with_store_map_option。
core的RegionInfo是唯一region输入；size与所有逻辑大小用MiB，keys保持有符号计数。

Observe先累加全局count、empty、size、kv、min(IA,kv)、columnar及keys；
empty复用core的size<=1且keys==0规则。过滤只作用于leader/peer分布与热点，多个条件AND。
leader可缺失，且即使不在meta peers内仍独立计入leader统计。peer顺序、重复store与
store0沿用core行为；非零store的witness仍计数但size/keys为0，TiFlash learner
不另按columnar值估算大小。各有符号总和与无符号负载总和使用Go式wrapping。

RegionStatInformer只要求本行为使用的get_hot_peer_stat，不扩张到hotness计算、调度
或缓存生命周期。HotPeerSnapshot是宿主真实HotPeer GetLoad已去噪并舍入后的四维值，
单位byte/s、key/s、query/s、CPU；聚合仅读取前三维。无快照不创建负载map项；
有快照的零负载创建零项。有限非负且可表示的值按Go uint64转换，截去小数。
NaN、负数与超范围浮点不是有效负载，Go对应转换依架构而异，不作跨架构保证。
后续HotPeer窗口实现通过快照接入此契约，无预设热度或调用次数。

StoreEngine只保存调用方注释，Observe不推断或覆盖。返回值拥有全部maps，不借用输入。
无协议、网络、存储、全局状态或后台任务；不新增依赖。独立region_totals_test由lib.rs登记。
Go只读运行证据与测试驱动记录见[任务14](../../docs/rust/state/task-14.md)。

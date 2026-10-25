# 基础集群观察契约

`BasicCluster` 对应 `cluster_informer.go` 的基础聚合接口：StoreSetInformer、
StoreSetController、RegionSetInformer。它只有 `stores()`、`regions()` 两个必需
借用入口，默认查询、控制方法直接委托已验收的 `astersql-pd-core`；直接为
core 的 `BasicCluster` 实现，无第二份 store/region 缓存。后续 SharedCluster、
SchedulerCluster、CheckerCluster 可在此基础上组合其各自依赖，本文件不预建这些接口。

宿主拥有 core 状态与更新时序。Region 更新使用原 `RegionsInfo` 的
`check_and_put_region` / `atomic_check_and_put_region`，同时维护 root 和角色索引；
调度 trait 不另建写入口。宿主取得独占访问完成更新后，source、target 消费者再次
借用同一 cluster，得到同一个 Arc payload。并发宿主应在自己的状态锁内借用该
cluster；此 trait 不安装锁、后台任务或 runtime，不依赖 proto、server 或存储后端。

缺失 store/region 查询返回 `None`；有错误返回的 store 控制保留
`StoreControlError::{StoreNotFound, SlowTrendEvicted}`，恢复操作保留 core 的
缺失条目忽略语义。Store 控制沿用 registry 内部锁和 copy-on-write，已持有的
store 快照不会被改写。Region staleness、重建和 overlap 规则属于 canonical
core；本任务重放 legacy stale 返回输入 region 的契约，不把拒绝当成更新成功。
`get_region_by_key` 在 core 扫描返回的候选上检查 `KeyRange::contains`，避免
把 hole 后的 region 当成命中；边界仍是左闭右开。

随机查询委托原有角色索引，显式传入调用方 index/shuffle 输入，不安装全局随机源。
默认查询保留 Go 的无序 store 集合、MiB 尺寸、signed range count 和 typed batch
hole error。领域 API 仅公开 core 类型与 Rust 标准类型；没有 feature 扩展。

运行证据见 [task-002.md](../../../docs/rust/schedule-execution/task-002.md)，
冻结输入/期望见 [task_002.json](../../../docs/rust/schedule-execution/fixtures/task_002.json)。
测试在 `task_002_test.rs` 独立注册：真实 store/region 对象、版本更新/拒绝、
source/target 同一状态、控制错误及 key hole/边界；期望来自 Go 调试运行输出和
现有 Go 测试实际通过的断言，不从 Rust 计算结果生成。保留快照的额外断言为
core copy-on-write 的源码契约检查，未冒称该断言也由 Go 调试输出直接证明。

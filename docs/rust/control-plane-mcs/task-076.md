# 任务076：恢复分组分配器监听

2026-11-06，基准`213abc35e`。状态：已完成。
[验收10原始回执](../server-api/acceptance/task-10.json.gz)保存本轮Go/Rust命令、输入指纹、
真实失败/通过、时间戳、compaction revision及持久状态；[汇总](../server-api/acceptance/task-10.md)。

canonical TSO crate提供GroupAllocatorWatch和GroupWatchConfig，由宿主提供共享MetaStore、
Tokio handle、取消令牌、clock及在协议边界编码的participant bytes。
复用ReconciledWatch完成一致快照、按revision续接、重连和compaction重建；先完整解码批次，
异步资源应用失败保留pending批次及已发布revision，修复源数据后可显式reload。
删除/撤出成员先取消并join真实allocator工作者，释放自己的election lease后发布集合。
缺失/删除默认组按Go恢复本地默认副本；元数据恢复不改写持久目录。

真实allocator复用原Oracle和Leadership，并使用canonical分组timestamp/election路径；
库不创建私有runtime或安装全局资源。稳定组每个任期维护自己的窗口和续租/更新工作。
transition资源不会提前发布独立时间戳窗口；74/75的拆分/合并时间戳交接仍属各自原拥有任务，
本轮仅推进验收10稳定目录所真正需要的72/73与watch生命周期，不将交接算法记作完成。
MCS网络服务装配仍由后续服务拥有任务接收这些公开库API。

本轮TDD：初始默认组没有真实allocator，0通过/1失败，exit101；修复后聚焦`mcs_076_`2通过，
`mcs_072_`2通过、`mcs_073_`1通过。全部5项均使用官方真实etcd。
覆盖Go相同动态PUT/DELETE输入、默认组恢复、实际发号、独立持久窗口、删除时primary键消失、
网络停服拒绝、同WAL恢复、离线期间删除/新增并compact后重建，以及非法批次不发布进度、修复后reload。
补录实际时间戳及compaction进度后`mcs_0`再次5通过。

TSO包级8单元+9集成通过，0失败/忽略；metadata、fmt、workspace check与架构守卫通过。
Go隔离测试4个目录/watch叶例通过，另1个实际TSO请求叶例和1个保留空间叶例通过；
Rust回放动态watch的同一11步输入，并补真实lease、故障、WAL和compaction状态。
HTTP验收另以同一候选端口验证实际服务allocator集合、发号primary和撤组清理。
完整workspace test依调度集中验收24，零doc-tests不计行为证据。

最终槽位1：`CARGO_TARGET_DIR=/Users/xiangmin/worker/code/pd/target/rust-slot-1`；
早期失败阶段亦动态使用槽位2，确切命令/退出码见回执；全部子进程结束后释放自有锁，保留缓存。
Go源码/模块1101项首尾一致；仅隔离构建既有测试，未注入failpoint、未改写Go输入。
所有plan.md与来源史诗只读。外部etcd证据不替代原生生产后端durability验收。

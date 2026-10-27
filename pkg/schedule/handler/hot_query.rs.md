# 热点与桶管理查询

`hot_query.rs` 扩展 canonical `Handler`，提供 `get_store/get_stores`、
`get_hot_regions/get_hot_stores/get_stores_loads/get_hot_buckets`。
`StoreInfo` 由真实 BasicCluster 提供；枚举后重新按 ID 查询，删除导致缺失时传播
`HotQueryError::StoreNotFound`。未初始化优先传播 `HandlerError::NotBootstrapped`。

统计由宿主按每次请求传入：`StoreStatInformer` 接受真实 StoresStats 或已取得的
StatisticsSnapshot，返回独立负载副本；`HotObservationInputs` 复用 Coordinator
已经验证的读写热点聚合、时间、learner 和 unknown-ID null 语义，不创建第二份缓存。
`get_hot_buckets` 借用真实 BucketWorkerClient，在宿主 runtime 中 await，采用当前
shared config 的 heat threshold。FIFO 保证此前接收的报告先于查询处理；查询取消、
满队列或关闭均保留类型化错误，不返回伪造空成功。宿主负责 worker 生命周期。
64位配置阈值到32位worker的转换饱和处理；超出所有桶degree上界的正阈值
仍先等待worker并传播错误，再返回空结果，不允许截断回绕。
这些各自拥有的快照不承诺跨 cache 的原子一致性，和 Go facade 的独立读取一致。

HotStoreStats 保留七个 Go JSON tag、空 map 的 omitempty，排除无负载/已删除 store
和 TiFlash compute；TiFlash write 使用 region 写入字节/键，其它 engine 使用 store
写入负载。标签判断复用 core 的精确比较。HotBucketsItem 将 key 转为大写十六进制，
保留空边界和 u64 原值，按 read bytes/keys、write bytes/keys 索引转换，QPS 不暴露。
这些领域投影可序列化，HTTP 状态、路由和认证留给适配层，不依赖 server/proto。

来源为只读 `handler.go:949–1091`、`coordinator.go`、`core/store.go`、
`statistics/buckets`。按用户授权，期望是源码推导，不是 Go 运行差分。
[证据](../../../docs/rust/schedule-execution/task-123.md)列出独立 fixture、真实状态更新、
错误路径、故意错误实现的失败结果及完整回归命令。

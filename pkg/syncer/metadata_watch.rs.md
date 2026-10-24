# 元数据监听恢复

`MetadataWatch` 是宿主主动驱动的原始元数据视图，与区域 `SyncRegions` 和
`HistoryIndex` 独立。`load` 接受唯一 `MetaStore`、字节 prefix、分页大小和宿主
取消令牌，复用 `astersql-pd-etcdutil::ReconciledWatch`：首页固定 revision，
其余页在同一 revision 读取，完整成功后发布 `BTreeMap<Vec<u8>, Entry>`。
`next_revision` 是最后成功发布 revision+1，不能用作区域历史索引。

`advance` 应用一个真实事务批次；压缩时复用既有全量读取及删除对账，移除新快照
已不存在的旧键。状态更新在独占借用下同步完成，无 await 或可失败回调，之后才
确认游标。Unavailable 不修改视图和游标，宿主决定重试间隔；重连从未确认游标
重放，避免已收到但断连丢弃的事件造成缺口。加载期间的压缩/错误返回给宿主，
不发布部分快照，宿主可重新调用 load；恢复加载失败保留原状态，下一次 advance
继续重载。取消为终态，换令牌不能恢复同一观察器。

本模块不创建任务、runtime、全局状态或协议类型。宿主负责循环、退避、取消和
join；drop 释放 watch。内存用量受观察 prefix 的键数量约束，适用于有限元数据。
不解析区域协议：区域键沿用 `ClusterPaths::region_path`，区域编解码与持久化留在
`StorageEndpoint`；消费者自行选择 prefix，本模块不替代区域缓存恢复。

独立测试 `metadata_watch_test.rs` 在 lib 显式注册，双后端重放分页间写入、
put/delete、丢弃真实IO响应、Unavailable、compaction/失败重载、取消与join；
另以真实etcd停止/同WAL重启验证重连。所有revision与HistoryIndex单独断言。

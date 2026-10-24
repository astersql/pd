# astersql-pd-syncer

本目录是Go pkg/syncer的canonical Rust crate。入口显式注册并重导出：

- `history_buffer`：有界历史与独立的 `HistoryIndex`。
- `history_window`：窗口增长、缩减与 `HistoryRetain`。
- `history_persist`：通过真实Base/MetaStore保存和恢复next index。
- `history_replay`：捕获结束索引内的增量分帧，或明确请求全量同步。

测试均在独立 `*_test.rs` 中显式登记；持久化测试复用现有etcd夹具。
生产依赖为workspace集中登记的core、storage-kv与thiserror；不依赖protobuf或传输。
宿主持有运行时、下游发送状态和锁；本crate不创建私有运行时或安装全局处理器。

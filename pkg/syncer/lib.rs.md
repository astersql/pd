# astersql-pd-syncer

本目录是Go pkg/syncer的canonical Rust crate。入口显式注册并重导出：

- `history_buffer`：有界历史与独立的 `HistoryIndex`。
- `history_window`：窗口增长、缩减与 `HistoryRetain`。
- `history_persist`：通过真实Base/MetaStore保存和恢复next index。
- `history_replay`：捕获结束索引内的增量分帧，或明确请求全量同步。
- `full_snapshot`：固定全量扫描与 history retain 边界。
- `full_catchup`：追赶固定上界，发送 completion，失败断开并要求重同步。
- `downstream`：独立发送任务、成功后推进各自索引、合并通知、背压及取消/join。

测试均在独立 `*_test.rs` 中显式登记；持久化测试复用现有etcd夹具。
生产依赖为workspace集中登记的core、storage-kv、thiserror与tokio；不依赖protobuf或传输。
宿主持有运行时并负责取消/join下游任务；本crate管理各流索引与历史锁，
不创建私有运行时或安装全局处理器。

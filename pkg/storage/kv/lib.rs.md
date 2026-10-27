# KV 兼容 facade 与 Etcd 适配器

保留既有 `astersql-pd-storage-kv` 的公开类型和 EtcdMetaStore 入口。中立类型直接重导出
`storage-domain::kv`，不创建包装版 MetaStore、MemoryStore 或第二份事务实现。
Etcd、tonic 与 RPC 错误映射仍只在本适配 crate；领域包改为依赖中立 crate。

全部原有单元、内存及真实 Etcd 集成测试仍注册在本 crate，验证原始行为与错误语义。
隐藏的校验、range 请求和取消辅助函数来自中立实现，供适配器复用而不复制算法。

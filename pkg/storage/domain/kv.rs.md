# KV 契约的编译归属

MetaStore、Base、CallbackTxn、MemoryStore 与 raw transaction 实现保留原文件及行为，
在本模块显式注册。原 `astersql-pd-storage-kv` 重导出相同类型，同时拥有 Etcd 实现；
领域包直接使用本 crate，避免通过兼容 facade 引入 transport。

三个隐藏的 public 验证/请求/取消辅助模块只供既有 Etcd 适配器复用校验，不包含 RPC
或 wire 类型；没有复制验证算法。完整原有 KV 测试仍在旧 facade 运行，覆盖真实内存
与 Etcd 后端；无需制造第二份测试期望。

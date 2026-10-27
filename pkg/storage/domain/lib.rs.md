# 中立存储边界

`astersql-pd-storage-domain` 是调度可依赖的存储契约与字节/JSON 实现。它不依赖
protobuf、prost、tonic、具体 Etcd 后端或 server/runtime。没有 feature 或私有 runtime。

`kv` 注册原 `storage/kv` 的同一份中立实现；原 KV crate 仅重导出这些类型并保留
Etcd 适配器，不再编译第二份 KV 逻辑。endpoint 的配置、规则、affinity、batch 方法
同样只在本 crate 编译；原 endpoint crate 保留 wire 实现并委托本 crate。

原 Go 字节编码、JSON、事务顺序、取消分类和分页契约不变。共享 MetaStore 的 Arc
是唯一后端状态；克隆 endpoint 只克隆后端句柄。已有 MemoryStore/Etcd 契约与新增
compat 验证检查类型身份、实际读写、错误和 wire codec 的兼容性。

任务156的 strict normal 传递依赖审计及 workspace 回归是最终验收入口。

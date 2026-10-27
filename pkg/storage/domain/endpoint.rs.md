# 字节与 JSON endpoint

从原 endpoint codec 分离的同一实现，提供 physical load、JSON 保存/恢复，以及配置、
调度器配置、规则、affinity 和 batch 事务入口。没有 protobuf 编码器或生成类型。

EndpointError 保留原错误分类和显示文本。适配器提供的 codec 错误以标准 Error trait
对象承载，不在中立契约中命名 prost 类型；实际 wire 编解码仍由原 endpoint 适配器执行。
StorageEndpoint 可克隆，只共享 MetaStore；wire adapter 的 region ID 游标仍由原适配器
独占，不进入中立 JSON 句柄。宿主 AsRef/Into 转换不重建后端或丢弃持久值。

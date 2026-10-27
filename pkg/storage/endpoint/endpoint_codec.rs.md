# wire endpoint 的兼容委托

原 StorageEndpoint 构造入口保留。它拥有原 wire codec 所需的 MetaStore 句柄与 region ID
游标，并将字节/JSON 方法通过 Deref 委托给同一后端的中立 endpoint。AsRef/Into 允许
宿主传给调度构造器，不引入领域到适配器的反向依赖，不重建后端。

protobuf 模块仍实际执行 prost 编解码，Go wire fixture 与真实后端测试保持启用。
错误分类与显示文本不变；底层 prost 错误装入中立 Error trait 对象，适配调用方仍可
在已依赖 prost 的边界检查其原始类型。没有在中立 crate 放入 wire 编解码逻辑。

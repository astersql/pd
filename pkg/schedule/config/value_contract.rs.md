# 配置值与标签契约

`value_contract.rs` 导出 `validate_labels`，接受借用的 `(key, value)` 序列，
按输入顺序先检查 key 再检查 value，返回首个错误。复用既有
`validate_label_key`，无需 protobuf 或另一份 StoreLabel 模型。
value 使用 Go `util.go` 的 ASCII 字母数字及 `-_./` 语法；空值合法，表示删除。

`lib.rs` 的既有 `decode_json` 现在要求完整 JSON 文档，尾随空白合法，尾随值报错。
所有浮点值的 JSON null 在新快照中保持零值；本 API 解码新快照，不承诺原地 patch。
JSON 错误保留 serde_json 的类型和位置，不要求复制 Go 解析器文案。
未增加 crate、feature、全局状态、I/O 或后台任务。

`config_values_test.rs` 在 lib.rs 显式注册；已有值模型、默认值、校验及 Clone 均复用。
Rust Clone 的拥有型集合会递归复制 scheduler args；Go Clone 复制 scheduler slice，
但嵌套 args 仍共享。Rust 提供更强的值隔离，不声称 Go 也递归复制 args。
运行 oracle、红绿证据与完整覆盖映射见
[批次 9 证据](../../../docs/rust/schedule/config/9-证据.md)。

# astersql-pd-schedule-types 入口

本目录是 Go `pkg/schedule/types` 的唯一 canonical Rust crate。`lib.rs` 重导出 `types.rs` 的领域 API，并显式注册独立 `types_contract_test.rs`，没有根 facade 业务副本。

生产依赖只有已有 `astersql-pd-kerneltype`，dev dependency serde_json 用于读取真实 Go debugger fixture。workspace member/dependency 集中注册，Cargo.lock 只新增本地包。

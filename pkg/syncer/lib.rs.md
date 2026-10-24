# astersql-pd-syncer

本目录是Go pkg/syncer的canonical Rust crate。入口只重导出history_buffer的领域API，
显式注册独立history_buffer_test.rs。生产依赖仅为已有core与thiserror；
serde_json仅用于测试读取只读Go运行fixture。workspace集中登记member/dependency并锁定依赖。
领域不依赖protobuf、传输或具体后端；本批不实现同步服务生命周期。

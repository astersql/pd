# statistics/utils crate

Canonical `astersql-pd-statistics-utils`, Rust 2024、默认 features，无生产依赖。
`kind` 模块的值与纯函数在 crate 根公开；`kind_test` 显式登记独立测试文件。
不接线 core、protobuf、transport、Registry 或运行时。
行为范围与兼容边界见 [kind.rs.md](kind.rs.md)，验收见
[任务证据](../../../docs/rust/state/task-01.md)。多维 TopN 集合通过 `TopN` / `TopNItem` 公开，测试模块 `topn_test` 显式登记，
见 [topn.rs.md](topn.rs.md)；TTL 属后续任务。

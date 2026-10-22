# PD Rust 叶子类型与通用算法迁移计划

目标：把来源主题 1 拆成可在聚焦会话中逐项验证的 Rust 迁移任务，保持 Go 行为、字节布局、并发与错误契约一致。

范围：迁移 `pkg/core/constant`、`pkg/keyspace/constant`、`pkg/mcs/utils/constant`、`pkg/slice`、`pkg/codec`、`pkg/balancer`、`pkg/movingaverage`、`pkg/window`、`pkg/btree`、`pkg/ratelimit`、`pkg/cache`、`pkg/core/rangetree`、`pkg/errs`、选定 `pkg/utils/*` 与 `pkg/versioninfo/kerneltype`。每个任务只迁移一个可测试行为或一个窄契约。

范围外：不切换 Go 生产调用方，不迁移调度/存储/服务端业务，不把 tonic/protobuf/etcd/axum 引入叶子领域 crate，不恢复已否决的原生 Raft 原型。

假设：Go oracle 固定于提交 `42a0da684cf28f16a48c74c54575b711031f3c5d`；现有 Rust workspace、ADR-001、依赖基线和移植指南可用；Go 继续运行并作为行为基准，直到更高主题完成调用链切换。

## 设计决策

- 比较了单一“大 utils crate”、按 Go package 建 canonical crate、直接用第三方 crate 替代三种路线；选择按稳定 Go package 建 crate，并仅在每个行为任务中复用可证明等价的标准库能力。
- Rust 文件与对应 Go package 同目录；crate 名使用 `astersql-pd-*`，根 facade 不复制实现。小工具也不合并成杂物 crate，以免扩大依赖面。
- `reflectutil` 用 serde 字段描述实现可观察 JSON tag 行为，不模拟 Go 运行时反射；`syncutil` 使用 Rust/Tokio 锁与取消，不复制 build-tag mutex；`errs` 只保存稳定领域错误，tonic/log 转换留在适配层。
- `btree` 保留 order-statistics、范围遍历和 copy-on-write clone 契约，不能仅用缺少这些语义的容器替代；缓存和范围树在该契约稳定后进入。
- 计划按实际 import 修正主题粗顺序：`syncutil` 先于 balancer/window/btree/cache/ratelimit，`btree` 先于 cache/rangetree，`errs` 基础先于 keyutil/ratelimit。
- 每批至多一个任务修改根 `Cargo.toml`；同批任务不写同一 Rust 源/测试文件。批次依赖以保守的整批完成为门槛。

## 架构说明

- 叶子 crate 依赖方向为 constants/slice/codec/typeutil/reflectutil -> syncutil/errs -> balancer/window/btree/jsonutil/keyutil -> cache/rangetree/ratelimit。
- 生产测试放在独立 `*_test.rs`，由 `lib.rs` 显式注册；所有可用 Rust 源加 `// Copyright 2026 AsterSQL.`。
- 时间、并发和淘汰测试使用受控时钟、屏障和真实任务；不以 mock 调用次数、sleep 碰运气、忽略测试或零测试作为完成证据。
- 每个首任务建立对应 crate、登记 workspace member 和集中依赖；后续任务只扩展该 crate。所有 Cargo 命令遵守任务文件中的共享槽位规则。

## 批次与并行约束

- 批次 1–26 采用流水线：每个 crate 内严格串行，不同 crate 仅在文件与依赖已分离后并行。
- 批次 24 后才启动 ratelimit，批次 25 后才启动 cache，批次 26 后才启动 rangetree；三者分别等待所需的 errs/syncutil/btree 证据。
- 批次 34 汇总覆盖与依赖审查，批次 35 执行主题级回归并按来源任务规则关闭史诗。
- 同一批最多 5 个任务，低于 Cargo 槽位上限 10；源码独立不代表可绕过各自任务依赖。

## 开发策略

- 对会改变行为的代码，使用失败验证-通过验证-重构。
- 在实施步骤前规划失败验证命令和预期失败。
- 将实施步骤限制在失败测试所证明的行为范围内。
- 测试真实行为，不验证模拟对象或实现细节。
- 每项先运行对应 Go oracle（若现有 Go 包没有测试，则逐项固定源码常量/布局），再写 Rust 失败测试；Rust 通过后执行该 crate 的全部测试。
- 每个任务完成时记录实际有效测试数、命令退出码、槽位与绝对 CARGO_TARGET_DIR；只有最终批次运行 workspace 级回归。


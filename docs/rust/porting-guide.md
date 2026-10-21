# Rust 移植指南

适用范围：后续 Rust 文件、Cargo 配置及跨语言兼容测试；现有 Go 规则继续适用。
[ADR-001](architecture.md) 定义架构，[依赖基线](dependencies.md) 定义依赖方向，
[仓库规则](../../AGENTS.md) 定义日常约束。当前任务只提供文档，以下 Cargo 检查
在对应 workspace/测试目标创建后执行，不表示现在可运行。

## 开始一个移植单元

1. 固定对应 Go 源码、测试、协议 commit 和可观察行为，列出成功、错误和故障场景。
2. 检查更深层 AGENTS.md；确定 canonical crate、公开领域类型与依赖方向。
3. 先规划能证明缺失行为的失败验证命令及预期结果，记录实际失败，再做最小实现。
4. 在独立 `*_test.rs` 文件编写真实行为测试，不能以 mock 调用次数或桩返回值代替契约。
5. 同一场景可选择 Go、Rust 或双实现；对比响应、状态码、序列化及状态变更。
   对 ID/TSO 比较唯一/单调等性质，对时间和随机字段说明归一化规则。
6. 获得通过证据后重构，再复核兼容性；未实现、被忽略或只有桩的测试不得算完成。

领域逻辑不依赖生成协议类型、网络框架或具体持久化引擎。proto 转换留在适配层。
长期任务必须有取消和 join；错误保留类型和稳定错误码；日志不得包含凭据。
实际可用的 Rust 源码顶部加入 `// Copyright 2026 AsterSQL.`；机械翻译和桩不能
仅凭添加标记被认定可用。生成代码应注明生成来源并按协议任务确定的策略管理。

## 测试与格式化

workspace 建立后，在根目录执行以下基线命令，并补充任务指定的失败/通过验证：

```bash
cargo metadata --format-version 1 --no-deps
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
```

生产测试置于独立文件；单元测试可通过显式 `#[path = "..._test.rs"]` 的测试模块接入，
集成测试在 Cargo target 中明确登记。不要把测试实现堆在生产文件中。
文件与相邻 Go 包并存，不创建第二份业务实现目录。依赖集中声明，统一 rustfmt 配置。
Cargo.lock、生成工具和协议来源必须固定后再使用 `--locked` 做可重复验证。

协议任务必须增加 wire 差分；MetaStore 任务必须运行跨后端契约；runtime 任务必须
覆盖独立进程与共享 Tokio handle 的启动、ready、失败清理和 shutdown。
持久化原型必须覆盖崩溃/恢复、日志与快照一致性、lease/选主故障，再决定生产引擎。

Go oracle 使用仓库自动管理 failpoints 的入口，具体包和测试名按本次迁移范围选择：

```bash
make gotest GOTEST_ARGS='./pkg/storage -count=1'
```

遵守 [Copilot failpoint 说明](../../.github/copilot-instructions.md)，测试后禁用注入；
运行时启用不会重放已结束的初始化。Rust 故障注入也必须覆盖实际到达注入点的时序，
与生产路径隔离。禁止提交 Go 生成的 failpoint 文件。

## 文档任务的验证

在仓库根执行：

```bash
rg -n "Rust 2024|library-first|MetaStore|不合仓|Go FFI|kvproto" docs/rust AGENTS.md
git diff --check
```

逐个检查 Markdown 相对链接指向现存文件/目录，拟建文件只在明确标注的代码块中列出。
审查决策是否单向、是否保留 Go 行为、是否越过当前仓库边界；文档变更无需运行 Go
生产测试。代码任务按其涉及的包和行为运行测试，报告命令、退出码和实际结果。

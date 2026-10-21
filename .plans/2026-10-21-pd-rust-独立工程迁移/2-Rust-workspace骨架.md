# 任务 2: 建立 Rust 2024 workspace 与薄二进制骨架

批次：【批次 2】 依赖批次 1

状态：未开始

目的：在 PD 独立仓库中建立可解析、可测试的 Rust workspace，并证明独立二进制与可嵌入 runtime 使用同一入口。

来源任务：无

预计会话范围：只建立根 workspace、facade、runtime 生命周期接口和空的 `pd-server` 进程壳，不移植 PD 业务或网络协议。

## 文件

- 新建：`Cargo.toml`
- 新建：`Cargo.lock`
- 新建：`rust-toolchain.toml`
- 新建：`rustfmt.toml`
- 新建：`.cargo/config.toml`
- 新建：`pkg/lib.rs`
- 新建：`pkg/runtime/Cargo.toml`
- 新建：`pkg/runtime/lib.rs`
- 新建：`pkg/runtime/node.rs`
- 新建：`pkg/runtime/node_test.rs`
- 新建：`cmd/pd-server/Cargo.toml`
- 新建：`cmd/pd-server/bin_main.rs`
- 新建：`cmd/pd-server/lib.rs`
- 新建：`cmd/pd-server/main.rs`
- 新建：`cmd/pd-server/main_test.rs`

## 上下文

- workspace 使用 edition 2024、resolver 3 和集中依赖版本。
- `bin_main.rs` 只能转发到 library 入口；核心 runtime 接收外部 Tokio handle 与取消令牌，独立模式才负责创建 runtime 和安装信号。
- crate 命名使用 `astersql-pd`、`astersql-pd-runtime`、`astersql-pd-cmd-pd-server`。
- 不添加未被骨架使用的推测性依赖。

## Cargo 共享槽位规则（仅 Rust/Cargo 任务）

1. 同一仓库的所有计划和任务共用仓库根目录下的 `target/rust-slot-1` 至 `target/rust-slot-10`，不得为任务、会话或计划另建构建目录，也不得按任务编号取模固定分配。
2. 每次构建/验证阶段开始前，创建仓库根目录下的 `target/rust-slot-locks`；依次用原子命令 `mkdir target/rust-slot-locks/slot-N.lock` 尝试领取槽位，`N` 为 1–10。仅 `mkdir` 成功者获得该槽位；这些相对路径命令从仓库根目录执行。
3. 领取成功后，在自己的锁目录中记录计划、任务、会话标识和持有进程 PID。同一槽位同时只允许一个持有者；已占用时尝试下一个，仅全部 10 个槽位都占用时等待并重试。
4. 将 `CARGO_TARGET_DIR` 设置为领取槽位的绝对路径，例如领取槽位 3 后，从仓库根目录执行 `export CARGO_TARGET_DIR="$PWD/target/rust-slot-3"`。示例编号不代表固定分配。
5. 所有 Cargo 构建、检查、测试以及调用 Cargo 的脚本和子进程均继承该设置，不得通过 `--target-dir` 或其他配置绕过槽位；生成的二进制也从该槽位定位。
6. 该阶段的 Cargo、脚本以及使用槽位产物的子进程全部结束后，删除自己写入的占用记录，并用 `rmdir` 释放自己的锁目录；保留槽位编译缓存供后续任务复用。
7. 正常结束、失败和中断均须清理自己的锁；在后台进程或其他使用槽位产物的进程仍运行时不得提前释放。规划清理步骤时覆盖这三种退出路径。
8. 遗留锁须先核实持有会话及相关进程已结束；无法确认则保持占用，不得删除其他活跃任务的锁、进程或缓存。
9. 生成或更新计划时，统一移除与这些规则冲突的单一 Rust 共享构建空间、逐任务独立目录和 Cargo 缓存全局独占窗口要求；仍须遵守源码文件占用、真实业务依赖和其他共享验证资源的互斥，不能仅因缓存分槽就把任务判定为可并行。
10. 最终验证记录包含实际槽位、绝对路径 `CARGO_TARGET_DIR`、确切命令、退出码和有效测试数量；Rust 代码修改后先运行 `cargo fmt --all`，再运行相应验证。编译成功或零测试不能替代行为验收证据。

## 测试计划

- 行为：独立入口和外部 runtime 装配都能启动同一个空节点并优雅关闭。
- 失败验证测试：`pkg/runtime/node_test.rs` 中新增共享 runtime 生命周期测试；实现前因 crate/workspace 不存在而失败。
- 失败验证命令：`cargo test -p astersql-pd-runtime --lib node_test -- --nocapture`
- 预期失败原因：workspace 和 `PdNodeBuilder` 尚不存在。
- 通过验证命令：`cargo test -p astersql-pd-runtime -p astersql-pd-cmd-pd-server --lib -- --nocapture`
- 模拟策略：只使用 Tokio 时间和取消令牌，不模拟 PD 业务。

## 步骤

1. 编写共享 runtime 生命周期失败测试。
2. 创建 workspace 与最小 crate manifests。
3. 实现 `PdNodeBuilder`、ready、shutdown 和薄二进制转发。
4. 运行格式化、metadata、聚焦测试和二进制 help/start-stop smoke test。

## 验证

- 运行：`cargo metadata --no-deps --format-version 1`
- 运行：`cargo test -p astersql-pd-runtime -p astersql-pd-cmd-pd-server --lib -- --nocapture`
- 预期：workspace edition 为 2024；生命周期测试不是零测试并全部通过。
- 所需证据：实际槽位、绝对 `CARGO_TARGET_DIR`、命令退出码、测试数量和独立/嵌入两条路径的事件顺序。

## 完成

骨架不得包含“返回固定成功响应”的 PD 业务桩；未实现能力必须明确返回 unsupported 或根本不暴露。


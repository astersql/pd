# 任务 104: 执行主题级 Go/Rust 回归并关闭来源任务

批次：【批次 35】依赖批次 34

状态：未开始

目的：主题级 Go oracle、Rust workspace fmt/check/test、metadata 与依赖审查全部通过后，按来源任务规则标记完成并删除来源任务文件

来源任务：`.plans/2026-10-22-pd-rust-渐进迁移主题/1-迁移叶子类型与通用算法.md`

预计会话范围：只处理一个行为、对应 Rust 源/测试和必要的 crate 连线；完成标准由聚焦红绿测试定义，适合一个短会话。

## 文件

- 修改：`.plans/2026-10-22-pd-rust-渐进迁移主题/1-迁移叶子类型与通用算法.md`
- 审查：`Cargo workspace 与全部范围内 Go 包`

## 上下文

- Go oracle 基准提交：`42a0da684cf28f16a48c74c54575b711031f3c5d`。
- 只有所有任务的当前证据齐全、无零测试伪通过、无 failpoint 残留且工作区差异已审查时才能关闭来源史诗。
- crate：`workspace`；任务内新增依赖必须集中到 workspace，禁止 proto、tonic、axum、etcd-client 进入叶子领域 crate。

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

## 验证计划

- 行为：主题级 Go oracle、Rust workspace fmt/check/test、metadata 与依赖审查全部通过后，按来源任务规则标记完成并删除来源任务文件
- 验证前提：逐项核对所有依赖批次的任务文件已删除或留有“已完成，待回归”的明确证据。
- 主要验证命令：`cargo test --workspace --locked`
- 周边验证命令：`cargo check --workspace --locked`
- Go oracle：`make gotest GOTEST_ARGS='./pkg/core/constant ./pkg/keyspace/constant ./pkg/mcs/utils/constant ./pkg/slice ./pkg/codec ./pkg/balancer ./pkg/movingaverage ./pkg/window ./pkg/btree ./pkg/ratelimit ./pkg/cache ./pkg/core/rangetree ./pkg/errs ./pkg/utils/jsonutil ./pkg/utils/keyutil ./pkg/utils/reflectutil ./pkg/utils/syncutil ./pkg/utils/timerutil ./pkg/utils/typeutil ./pkg/versioninfo/kerneltype -count=1'`
- 失败判据：实现或证据未齐时，覆盖矩阵/主题回归会显示缺项、依赖越界或命令失败。
- 模拟策略：不模拟；读取真实 Cargo metadata/tree、真实测试结果与工作区差异。

## 步骤

1. 核对依赖批次的完成证据和当前工作区状态。
2. 运行主要验证命令，记录任何缺失、失败或零测试结果。
3. 只补齐本任务指定的证据/文档/来源任务状态，不掩盖失败。
4. 运行周边验证、`cargo fmt --all -- --check` 与 `git diff --check`。
5. 审查依赖方向、有效测试数和 failpoint/临时产物状态。

## 验证

- 运行：`cargo fmt --all -- --check`
- 运行：`cargo test --workspace --locked`
- 运行：`cargo check --workspace --locked`
- 预期：聚焦测试和 crate/工作区检查退出码均为 0，至少执行一个有效行为测试；文档审计任务须列出真实检查项数量。
- 所需证据：失败验证的实际失败、通过验证的命令与退出码、有效测试数量、实际槽位、绝对 `CARGO_TARGET_DIR`、Go oracle 结果或源码逐项对照、`git diff --check` 结果。

## 完成

完成 `执行主题级 Go/Rust 回归并关闭来源任务` 的当前证据并更新 `.plans/2026-10-22-pd-rust-渐进迁移主题/1-迁移叶子类型与通用算法.md`；不得用历史运行结果关闭任务。


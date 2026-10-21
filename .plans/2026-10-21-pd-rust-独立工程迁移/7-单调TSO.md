# 任务 7: 实现可恢复的单调 TSO

批次：【批次 7】 依赖批次 6

状态：未开始

目的：实现与 Go PD 对齐的时间戳持久化、窗口预留、时钟回退保护和并发分配行为。

来源任务：无

预计会话范围：只处理 TSO oracle 及其持久化/时钟语义，不接入 gRPC 服务、keyspace group 或完整 MCS TSO。

## 文件

- 新建：`pkg/tso/Cargo.toml`
- 新建：`pkg/tso/lib.rs`
- 新建：`pkg/tso/oracle.rs`
- 新建：`pkg/tso/oracle_test.rs`
- 新建：`pkg/tso/restart_test.rs`
- 修改：`Cargo.toml`

## 上下文

- 先读取现有 `pkg/tso` 持久化窗口、物理/逻辑部分、guard 和更新循环，不允许用单一 `AtomicU64` 替代。
- TSO 只允许 leader 分配；leadership lease 丢失后必须立即拒绝请求。
- 时钟作为 trait 注入以验证回退和停滞，生产实现使用系统时钟。

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

- 行为：并发请求、进程重启、leader 切换、物理时钟回退和逻辑位溢出时，已发放 TSO 始终严格递增且不重复。
- 失败验证测试：`oracle_test.rs` 和 `restart_test.rs` 先覆盖上述五类场景。
- 失败验证命令：`cargo test -p astersql-pd-tso --lib -- --nocapture`
- 预期失败原因：TSO crate 和持久化窗口算法尚不存在。
- 通过验证命令：`cargo test -p astersql-pd-tso -- --nocapture`
- 模拟策略：时钟与 leader lease 可确定性注入；MetaStore 事务使用已通过契约的真实后端。

## 步骤

1. 从 Go 实现和测试提取 TSO 位布局、窗口和更新不变量。
2. 先写并发、重启、切主、回退和溢出失败测试。
3. 实现 oracle、持久化窗口和 leader guard。
4. 在内存和 Etcd MetaStore 上复用场景。

## 验证

- 运行：`cargo test -p astersql-pd-tso -- --nocapture`
- 预期：所有场景严格递增；lease 丢失路径明确失败而不是继续发号。
- 所需证据：实际槽位、命令退出码、测试数量、每个边界场景的首尾 TSO 和持久化值。

## 完成

只实现 classic 单组 TSO 基础；keyspace group、MCS TSO 和跨 DC global TSO 应分别建立后续任务。


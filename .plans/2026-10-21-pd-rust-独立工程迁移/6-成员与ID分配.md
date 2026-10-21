# 任务 6: 实现成员管理与 ID 分配

批次：【批次 6】 依赖批次 5

状态：未开始

目的：基于真实 MetaStore 实现可恢复的 member bootstrap、leader 所有权和并发安全的全局 ID 分配。

来源任务：无

预计会话范围：只处理 member、leadership 和 ID allocator 三个紧密相关的持久化行为，不接入网络服务、TSO、region heartbeat 或调度器。

## 文件

- 新建：`pkg/member/Cargo.toml`
- 新建：`pkg/member/lib.rs`
- 新建：`pkg/member/member.rs`
- 新建：`pkg/member/member_test.rs`
- 新建：`pkg/id/Cargo.toml`
- 新建：`pkg/id/lib.rs`
- 新建：`pkg/id/allocator.rs`
- 新建：`pkg/id/allocator_test.rs`

## 上下文

- member 和 allocator 必须接收 `Arc<dyn MetaStore>` 与取消令牌，不得依赖进程全局单例。
- leadership 必须由 lease 和 revision 保护；丢失 lease 后不得继续分配 ID。
- ID 分配需要批量预留、并发唯一、跨重启不回退，并保留 Go key layout 的兼容决策。

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

- 行为：成员启动与 leader lease 正确落盘；并发和重启后的 ID 永不重复或回退。
- 失败验证测试：分别先写 member bootstrap、lease loss、并发 ID 和 restart 场景。
- 失败验证命令：`cargo test -p astersql-pd-member -p astersql-pd-id --lib -- --nocapture`
- 预期失败原因：对应 crates 和 runtime 接线尚不存在。
- 通过验证命令：`cargo test -p astersql-pd-member -p astersql-pd-id -- --nocapture`
- 模拟策略：MetaStore 契约测试后端必须复用；lease 时钟可注入，持久化事务不可 mock。

## 步骤

1. 为 member bootstrap、leadership lease 和 ID allocator 写失败测试。
2. 实现 member 持久化格式和 lease 所有权状态机。
3. 实现批量 ID 分配及冲突重试。
4. 使用内存和 Etcd MetaStore 运行相同模块测试。

## 验证

- 运行：`cargo test -p astersql-pd-member -p astersql-pd-id -- --nocapture`
- 预期：lease 丢失立即停止写入；并发及重启场景的 ID 集合唯一且严格前进。
- 所需证据：实际槽位、命令退出码、测试数量、lease 丢失事件以及重启前后 ID 边界。

## 完成

仅通过内存全局单例或 mock 事务的测试不能完成本任务；EtcdMetaStore 契约路径必须有真实验证。


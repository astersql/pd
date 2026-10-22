# 任务 8: 迁移控制面服务与 MCS

批次：【批次 8】依赖批次 7

状态：未开始

目的：迁移 keyspace、GC、安全恢复等扩展控制面能力，并按依赖顺序实现可独立或嵌入运行的 MCS 服务。

来源任务：对应本主题计划的主题 8。

预计会话范围：这是多服务迁移史诗。业务能力和每个 MCS 服务都必须建立独立包级计划；不得在一个 crate 中复制 monolith server 或共享可变全局状态。

## 文件

- 迁移源：`pkg/keyspace/`
- 迁移源：`pkg/gc/`
- 迁移源：`pkg/replication/`
- 迁移源：`pkg/encryption/`
- 迁移源：`pkg/unsaferecovery/`
- 迁移源：`pkg/mcs/`
- 迁移源：`pkg/mcs/utils/`
- 迁移源：`pkg/mcs/registry/`
- 迁移源：`pkg/mcs/discovery/`
- 迁移源：`pkg/mcs/server/`
- 迁移源：`pkg/mcs/metastorage/`
- 迁移源：`pkg/mcs/tso/`
- 迁移源：`pkg/mcs/router/`
- 迁移源：`pkg/mcs/resourcemanager/`
- 迁移源：`pkg/mcs/scheduling/`
- 迁移源：`pkg/utils/tsoutil/`
- 修改：`Cargo.toml`
- 测试：对应 Go 测试、`tests/integrations/mcs/` 和新增 Rust 服务测试

## 上下文

- 推荐业务顺序：replication/encryption/unsaferecovery 中已满足依赖者 -> keyspace -> GC；推荐 MCS 顺序：utils/registry/discovery/server -> metastorage -> TSO -> router -> resource manager -> scheduling。
- 每个 MCS 必须复用领域 crate、storage 契约和公共 runtime，不得复制调度、TSO、keyspace 或 MetaStore 状态机。
- 服务发现和 watch 必须覆盖 compaction、重连、主从切换；独立部署与嵌入模式使用同一实现。

## Cargo 共享槽位规则

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

- 测试驱动开发说明：每个业务包和服务单独定义失败测试；主题层验证服务发现、跨服务协议、选主切换和独立/嵌入双模式。
- Go oracle：`make gotest GOTEST_ARGS='./pkg/keyspace/... ./pkg/gc ./pkg/replication ./pkg/encryption ./pkg/unsaferecovery ./pkg/mcs/... ./pkg/utils/tsoutil -count=1'`
- Rust 回归：`cargo fmt --all -- --check && cargo test --workspace --locked`
- 集成验证：`make -C tests/integrations gotest GOTEST_ARGS='./mcs/... -count=1'`
- 模拟策略：真实 MetaStore、listener 和服务发现；KMS 只在清晰凭据边界使用官方测试替身并验证脱敏。

## 步骤

1. 按真实依赖为控制面业务包逐个创建迁移计划，先完成可复用领域服务。
2. 迁移 MCS 公共注册、发现、生命周期和配置骨架。
3. 按 metastorage、TSO、router、resource manager、scheduling 顺序逐服务迁移并运行协议差分。
4. 验证服务拆分/合并部署、watch 恢复、主从切换、跨服务调用和 shutdown。

## 验证

- 运行：上述 Go oracle、Rust workspace 与 MCS 集成命令。
- 预期：全部退出码为 0；每个服务独立和嵌入模式行为一致；compaction/重连/切主后状态收敛；无领域逻辑副本。
- 所需证据：逐包/逐服务计划、服务矩阵、协议差分、故障恢复记录、依赖树、退出码和有效测试数。

## 完成

范围内业务能力与 MCS 全部通过独立/嵌入验收后标记 `已完成`；提交实际变更并删除本任务文件。

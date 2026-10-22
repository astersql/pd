# 任务 69: 迁移 DeleteMin 与 DeleteMax

批次：【批次 19】依赖批次 18

状态：已完成，待回归

目的：连续删除两端直到空树，返回值和 found 标志一致

来源任务：`.plans/2026-10-22-pd-rust-渐进迁移主题/1-迁移叶子类型与通用算法.md`

预计会话范围：只处理一个行为、对应 Rust 源/测试和必要的 crate 连线；完成标准由聚焦红绿测试定义，适合一个短会话。

## 文件

- 修改：`pkg/btree/lib.rs`
- 修改：`pkg/btree/lib.rs`
- 新建或修改：`pkg/btree/btree_test.rs`
- Oracle：`pkg/btree/btree_generic.go`

## 上下文

- Go oracle 基准提交：`42a0da684cf28f16a48c74c54575b711031f3c5d`。
- 依赖 syncutil；保留排序、范围迭代、order-statistics、copy-on-write clone 与 freelist 行为，不能仅用无序容器替代。
- crate：`astersql-pd-btree`；任务内新增依赖必须集中到 workspace，禁止 proto、tonic、axum、etcd-client 进入叶子领域 crate。

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

- 行为：连续删除两端直到空树，返回值和 found 标志一致
- 失败验证测试：`pkg/btree/btree_test.rs` 中的 `delete_min_max`
- 失败验证命令：`cargo test --locked -p astersql-pd-btree delete_min_max -- --nocapture`
- 预期失败原因：新增测试先引用尚未实现的符号/分支或命中现有行为差异，因此编译失败或断言失败。
- 通过验证命令：`cargo test --locked -p astersql-pd-btree delete_min_max -- --nocapture`
- crate 回归命令：`cargo test --locked -p astersql-pd-btree --lib`
- Go oracle：`make gotest GOTEST_ARGS='./pkg/btree -run "^(TestDeleteMinG|TestDeleteMaxG)$" -count=1'`
- 模拟策略：除明确的单调时钟、取消令牌或外部 Registry 边界外使用真实值与真实并发；不得模拟内部调用次数。

## 步骤

1. 阅读 Go 源码、对应 Go 测试和直接 Rust 调用需求，只固定本任务行为。
2. 在独立 `*_test.rs` 中编写失败验证；首任务先建立最小 Cargo manifest 与测试注册，不实现生产行为。
3. 运行 Go oracle（如有）与失败验证命令，确认失败原因精确匹配。
4. 实现让测试通过的最小 Rust 生产代码；不提前迁移本任务之外的 API。
5. 运行 `cargo fmt --all`、聚焦测试和 crate 回归；审查依赖树与差异后再提交。

## 验证

- 运行：`cargo fmt --all -- --check`
- 运行：`cargo test --locked -p astersql-pd-btree delete_min_max -- --nocapture`
- 运行：`cargo test --locked -p astersql-pd-btree --lib`
- 预期：聚焦测试和 crate/工作区检查退出码均为 0，至少执行一个有效行为测试；文档审计任务须列出真实检查项数量。
- 所需证据：失败验证的实际失败、通过验证的命令与退出码、有效测试数量、实际槽位、绝对 `CARGO_TARGET_DIR`、Go oracle 结果或源码逐项对照、`git diff --check` 结果。

## 完成

完成 `pkg/btree/lib.rs` 中的本任务行为和 `pkg/btree/btree_test.rs` 的独立测试；只在上述红绿证据、crate 回归和差异审查齐全后标记完成。


## 本次验证证据（2026-10-22）

- 批次 18：48、74 的实现分别已提交 `2c33c6528`、`f7aa5e4af`，68 已提交 `9588d7396` 且任务文件已删除；保留前置文件中的待回归原因是当时 BTree delete 缺失，现已补齐。
- 红：`cargo test --locked -p astersql-pd-btree delete_min_max -- --nocapture` 退出 101，E0599 精确指向缺失的 delete_min/delete_max。
- 绿：相同命令退出 0，1 项有效测试通过，覆盖 4 种 degree × 2 种比较顺序 × 3 种删除模式（每组 512 项）；校验完整返回值、空树 None、len/min/max/get、删除至空后重新插入，值包含非 Copy 的 String。
- `cargo test --locked -p astersql-pd-btree --lib` 退出 0，7 项通过，0 ignored。
- `cargo fmt --all`、`cargo fmt --all -- --check`、`cargo metadata --format-version 1 --no-deps`、`cargo tree --locked -p astersql-pd-btree` 和 `git diff --check` 均退出 0。BTree 无外部依赖，无 Cargo 配置变更。
- Go oracle：`make gotest GOTEST_ARGS='./pkg/btree -run "^(TestDeleteMinG|TestDeleteMaxG)$$" -count=1'` 退出 0，2 项通过，make 已自动禁用 failpoints；双美元用于 Make 转义。
- `cargo check --workspace --locked` 退出 101：并行任务未跟踪文件 pkg/movingaverage/time_median.rs:19 向 MedianFilter::new(isize) 传 i64。
- `cargo test --workspace --locked` 退出 101：并行任务 pkg/utils/keyutil/util_test.rs 引用尚未实现的 build_key_range_maps 等函数。两处属于其他进行中的任务，本次 BTree 聚焦和全部 crate 测试通过，保留本文件待统一回归。
- 红灯使用动态领取槽位 1，CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1；绿灯及补充检查使用动态领取槽位 2，CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2。每阶段退出 trap 均已移除自己的 owner 和锁目录，保留缓存。
- Go 源码逐项对照：沿首/尾子树下降，最小子树先借位/合并；删除成功递减长度；根空时提升唯一子节点；Rust Option<T> 对应 Go (T, found)，无需 T: Default 或 Clone。

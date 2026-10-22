# 任务 22: 建立 reflectutil crate 并迁移子标签到完整路径解析

批次：【批次 7】无

状态：未开始

目的：嵌套 JSON 字段描述可从子 tag 返回完整路径，未知字段为空

来源任务：`.plans/2026-10-22-pd-rust-渐进迁移主题/1-迁移叶子类型与通用算法.md`

预计会话范围：只处理一个行为、对应 Rust 源/测试和必要的 crate 连线；完成标准由聚焦红绿测试定义，适合一个短会话。

## 文件

- 新建：`pkg/utils/reflectutil/Cargo.toml`
- 修改：`Cargo.toml`
- 新建：`pkg/utils/reflectutil/lib.rs`
- 新建：`pkg/utils/reflectutil/tag_test.rs`
- Oracle：`pkg/utils/reflectutil/tag.go`

## 上下文

- Go oracle 基准提交：`42a0da684cf28f16a48c74c54575b711031f3c5d`。
- Rust 无 Go 运行时反射；以 serde 字段描述/Value 路径实现同一可观察 JSON tag 行为，不暴露伪反射 API。
- crate：`astersql-pd-reflectutil`；任务内新增依赖必须集中到 workspace，禁止 proto、tonic、axum、etcd-client 进入叶子领域 crate。

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

- 行为：嵌套 JSON 字段描述可从子 tag 返回完整路径，未知字段为空
- 失败验证测试：`pkg/utils/reflectutil/tag_test.rs` 中的 `full_tag_by_child_tag`
- 失败验证命令：`cargo test --locked -p astersql-pd-reflectutil full_tag_by_child_tag -- --nocapture`
- 预期失败原因：先建立 manifest、测试注册和失败测试但不写生产实现时，目标符号缺失或断言失败。
- 通过验证命令：`cargo test --locked -p astersql-pd-reflectutil full_tag_by_child_tag -- --nocapture`
- crate 回归命令：`cargo test --locked -p astersql-pd-reflectutil --lib`
- Go oracle：`make gotest GOTEST_ARGS='./pkg/utils/reflectutil -run "^(TestFindJSONFullTagByChildTag)$" -count=1'`
- 模拟策略：除明确的单调时钟、取消令牌或外部 Registry 边界外使用真实值与真实并发；不得模拟内部调用次数。

## 步骤

1. 阅读 Go 源码、对应 Go 测试和直接 Rust 调用需求，只固定本任务行为。
2. 在独立 `*_test.rs` 中编写失败验证；首任务先建立最小 Cargo manifest 与测试注册，不实现生产行为。
3. 运行 Go oracle（如有）与失败验证命令，确认失败原因精确匹配。
4. 实现让测试通过的最小 Rust 生产代码；不提前迁移本任务之外的 API。
5. 运行 `cargo fmt --all`、聚焦测试和 crate 回归；审查依赖树与差异后再提交。

## 验证

- 运行：`cargo fmt --all -- --check`
- 运行：`cargo test --locked -p astersql-pd-reflectutil full_tag_by_child_tag -- --nocapture`
- 运行：`cargo test --locked -p astersql-pd-reflectutil --lib`
- 预期：聚焦测试和 crate/工作区检查退出码均为 0，至少执行一个有效行为测试；文档审计任务须列出真实检查项数量。
- 所需证据：失败验证的实际失败、通过验证的命令与退出码、有效测试数量、实际槽位、绝对 `CARGO_TARGET_DIR`、Go oracle 结果或源码逐项对照、`git diff --check` 结果。

## 完成

完成 `pkg/utils/reflectutil/lib.rs` 中的本任务行为和 `pkg/utils/reflectutil/tag_test.rs` 的独立测试；只在上述红绿证据、crate 回归和差异审查齐全后标记完成。


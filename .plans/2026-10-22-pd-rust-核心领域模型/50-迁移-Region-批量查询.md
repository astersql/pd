# 任务 50: 迁移-Region-批量查询

批次：【批次 50】依赖批次 49（并继承此前全部依赖）

状态：未开始

目的：QueryRegions 按 keys、prevKeys 和 IDs 返回对应结果，重复与缺失输入、128 分批边界正确。

来源任务：`.plans/2026-10-22-pd-rust-渐进迁移主题/2-迁移核心领域模型.md`，主题 2。

预计会话范围：只处理本文件列出的单一验收故事，通常为一次聚焦会话；不把相邻模块、基础设施重构或其他编号任务并入。线性顺序为任务 1 → 69，本任务完成后进入下一批次；本批次不并行。

## 文件

- 新建或按前序实际结果修改：`pkg/core/regions_query.rs`，只实现本故事；新建 `pkg/core/regions_query_test.rs`。
- 修改：`pkg/core/lib.rs`，显式注册生产模块和独立测试模块，保留前序 API。
- 新建：`pkg/core/rust_core_task_50_oracle_test.go`（与 Go 原包同 package，可访问内部行为）和 `pkg/core/rust-fixtures/core_task_50.json`。
- fixture 解析使用 workspace 已固定的 `serde`、`serde_json`，只在需要解析的 crate 中登记为 dev-dependencies，领域正常依赖不为测试引入序列化器。
- 如需增加依赖，仅修改对应 `pkg/core/Cargo.toml`，复用 workspace.dependencies；锁文件只有真实依赖变化时更新。

## 上下文

- 阅读根 `AGENTS.md`、`.github/copilot-instructions.md`、`docs/rust/architecture.md`、`docs/rust/porting-guide.md` 与依赖基线；重新检查更深层说明。
- Go 来源：`pkg/core/region.go`。基准 `0820f0b67b0e49b289b84fdf66b7b8fb178afe82`；实现前复核该范围后续差异。
- 已有 Cargo workspace 与 `pkg/core/rangetree`、constant、ratelimit、movingaverage 等实现；API 缺口需用源码及测试确认。不得改用 proto 类型绕过缺口。

- 计划声明的 Rust 模块是拟建文件，后续任务不得建立第二份同名领域模型。扩展字段放在原拥有类型模块，行为可用 impl 分散到此任务模块；因此即使文件不同也串行。
- 本任务 crate 为 `astersql-pd-core`，使用默认 features，不新增 feature 开关。server 任务只增加窄适配，不接入网络服务或更改既有 PdServices。

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

## 测试计划

- 行为：QueryRegions 按 keys、prevKeys 和 IDs 返回对应结果，重复与缺失输入、128 分批边界正确。
- 失败验证测试：`pkg/core/regions_query_test.rs` 中以 `core_task_50_` 开头的真实行为测试；至少覆盖正常、边界和异常或无候选路径。
- Go oracle：新增 `TestRustCoreTask50Oracle`，读取 `pkg/core/rust-fixtures/core_task_50.json` 的输入，调用真实 Go 符号并断言期望；Rust 读取同一 fixture，独立计算后比较同一预期。首次 fixture 的期望必须来自当前 Go 运行证据。
- Go 命令：`make gotest GOTEST_ARGS='./pkg/core -run "^(TestQueryRegions.*|TestRustCoreTask50Oracle)$" -count=1 -v'`。只接受实际执行匹配用例；已有 Go 测试不足时由新增 oracle 覆盖本故事。
- 失败验证命令：`cargo test --locked -p astersql-pd-core --lib core_task_50 -- --nocapture`。
- 预期失败原因：对应真实行为尚未实现或结果不符合 oracle；bootstrap 可先因缺少公开类型失败。禁止把无效 manifest、未注册测试、环境错误和零测试算红灯。
- 通过验证命令：`cargo test --locked -p astersql-pd-core --lib core_task_50 -- --nocapture`，再执行 `cargo test --locked -p astersql-pd-core --lib`。
- 模拟策略：使用真实领域值、限流器和内存索引；仅对时钟、随机或调度交错注入确定性控制，不 mock protobuf getter 或锁调用次数。时间场景显式列出每次推进的时刻，随机场景比较候选性质。

## 步骤

1. 读源码及契约清单，确认实际依赖已提供所需行为。固定本故事输入输出；为新增 Go oracle 和 Rust 测试准备同一份小 fixture。
2. 串行运行 Go oracle 并保存输出。成功、失败和中断均用清理保证 failpoint-disable；确认禁用后才编辑或执行 Cargo、git、格式化等命令，不与别的 Go failpoint 测试并行。
3. 登记独立 Rust 测试模块后领取槽位，运行失败命令并确认真实缺失行为。建 crate 的任务只做使测试可运行所需的最小 manifest 和注册。新增 crate 或依赖后，若锁文件尚未登记，先以本任务通过命令去掉 `--locked` 运行一次以同步锁文件，审查只包含所需依赖变更；这次准备运行不作为失败证据，随后重新执行完整 `--locked` 失败验证。
4. 实现本故事最小生产代码；已有类型字段及选项只扩展当前故事需要的部分，禁止提前创建未验证的其他方法。新增可用 Rust 源码添加 AsterSQL 2026 版权。
5. 先 `cargo fmt --all`，审查只保留本任务文件差异，再运行通过命令与 crate 周边测试；同一工作区不得与其他任务格式化或改写源码并行。
6. 执行 `cargo metadata --format-version 1 --no-deps`、`cargo fmt --all -- --check`、`cargo check --workspace --locked`、`cargo test --workspace --locked`，记录环境或无关失败与聚焦测试结果的区别；全部 Cargo 命令都继承槽位。
7. 需要重构时保持测试通过；运行 `git diff --check`，清理自己的锁并保留缓存，按完成规则提交和交接。

## 验证

- 所需证据：Go oracle 与共享 fixture、实际红灯原因和输出、绿灯命令与退出码、至少一个有效 Rust 行为测试且无 ignore，完整用例数量、审查过的 API 差异、实际槽位和绝对 CARGO_TARGET_DIR。
- 生产实现不得把 fixture 作为返回结果；Go oracle 只用于测试。不更改线上 Go 行为或禁用检查以对齐 Rust。
- 调整 Go 测试后 gofmt 对应文件；提交前确认没有 failpoint 生成文件、无关格式化差异或其他任务变更。

## 完成

- 只有本任务所需证据齐全才能标记 `已完成`；最终回复列出确切文件、关键符号、行为、命令、退出码与有效测试数。
- 有实际代码、测试、配置或文档变更时使用 `$git-commit` 技能提交本任务，排除其他任务和工作区原有变更；只有本任务文件状态更新或删除时不调用该技能、不单独提交。
- 完成后按执行提示删除本任务文件；无关环境原因使验证无法执行时只能记 `已完成，待回归`，明确缺少哪项证据并保留文件。
- 尽力解决相关阻塞，但不把独立行为并入超大任务。无法突破时仅在本任务文件标记 `已阻塞`，记录具体缺口和已运行检查。来源史诗只有最终回归任务可以关闭；两个 plan.md 均只读。

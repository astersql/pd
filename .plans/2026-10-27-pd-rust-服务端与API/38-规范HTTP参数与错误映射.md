# 任务 38: 规范HTTP参数与错误映射

批次：【批次 38】依赖批次 37（含此前传递依赖）

状态：未开始

目的：统一JSON、URL参数、空值、Content-Type及HTTP错误体，非法请求有Go等价状态且不泄漏内部错误。

来源任务：主题7“补齐PD服务端与API”；来源 `.plans/2026-10-22-pd-rust-渐进迁移主题/7-补齐PD服务端与API.md`。

预计会话范围：仅处理本文件描述的一个行为边界及局部接线、失败测试与证据；依赖满足后无需阅读其他编号任务。不得顺带重写相邻子系统。

## 执行顺序与依赖

- 线性顺序：任务1至任务102；本任务为第38步。依赖批次 37（含此前传递依赖）。
- 并行批次：本批次仅本任务；所有批次串行，因共享lib/router/Cargo.lock、全局格式化与提交可能冲突。Cargo缓存分槽不解除源码及外部差分环境互斥。
- 依赖产物：只读 `plan.md`；前三个盘点任务产出的清单按实际存在顺序读取。主题6证据须经任务1确认；必要上游能力缺失时先修复或记录阻塞，不降级为固定响应。

## Go 只读硬约束

- Go 仅供只读参考。禁止新增、修改、删除、重命名任何 `.go` 文件（含生产代码、测试、oracle、生成文件及临时目录中的 Go 工具）；`go.mod`、`go.sum`、`go.work`、`go.work.sum` 同样只读。所有新增实现与测试使用 Rust，必要的非 Go 验证脚本和证据文档须在任务范围内。
- 禁止通过生成器、格式化、依赖整理、failpoint 注入或构建脚本间接改写 Go；不得运行会改写 Go 的 make 测试入口、failpoint 启停命令、`go generate`、`go mod tidy` 或 Go 格式化写入命令。不得以“只增加测试/工具”“临时修改后还原”或扩大任务范围绕过约束。
- Go oracle 仅复用已有只读源码、测试、fixtures、已保存输出或已有二进制；运行已有 Go 工具/测试前，必须确认整个调用链不写上述文件且不需要 failpoint 注入。需要 failpoint 的测试不得直接裸跑来替代验证；无法只读取得必要运行证据时，记录具体缺口并按任务阻塞规则处理，不降低验收标准，不将源码推导冒充运行差分。
- 开始与结束时核对上述文件的 Git 状态及内容，保留用户已有变更；本任务不得产生任何 Go 文件或 Go 模块文件差异。本约束优先于计划、执行技能或验证要求中任何相冲突的写入指令。


## 文件

- 拟建：`pkg/server/http_contract.rs`、独立测试 `pkg/server/http_contract_test.rs`、证据 `docs/rust/server-api/task-038.md`。
- 局部接线可修改：`pkg/server/lib.rs`、`pkg/server/grpc.rs`、`pkg/server/http.rs`、`pkg/server/Cargo.toml`。
- 仅实际需要依赖时修改根 `Cargo.toml`/`Cargo.lock`；统一workspace.dependencies，不新建平行服务。
- 测试在lib.rs显式注册为独立模块；已有文件优先扩展，禁止复制领域实现。

## 上下文

- 只读Go oracle与现有实现：`pkg/utils/apiutil/apiutil.go`、`server/api/middleware.go`、`pkg/server/http.rs`。目录为扫描范围，不是可写Go文件。
- 现有生成协议固定kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`；不得以浮动协议或默认tonic桩代替Go契约。
- 已有能力先核查实际公开API并复用；下列新文件均为本计划拟建，不代表当前存在。

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

- crate：`astersql-pd-server`；features：默认集合，不新增可选feature；过滤前缀：`task_038_`。
- 行为：统一JSON、URL参数、空值、Content-Type及HTTP错误体，非法请求有Go等价状态且不泄漏内部错误。
- 失败验证测试：在 `pkg/server/http_contract_test.rs` 新增 `task_038_behavior`；按上面单一行为补成功及边界/故障案例，测试名全部使用该前缀。
- 失败验证命令：`cargo test --locked -p astersql-pd-server --lib task_038_ -- --nocapture`。
- 预期失败原因：当前HTTP仅有两个固定路由。测试须先编译可运行并失败于行为断言；若缺少新接口导致编译失败，记录后先打通最小接口，再取行为失败证据，不能仅以编译错误证明缺陷。
- 通过验证命令：`cargo test --locked -p astersql-pd-server --lib task_038_ -- --nocapture`；`cargo test --locked -p astersql-pd-server -- --nocapture`。
- 模拟策略：网络路径使用真实TCP/tonic/HTTP和同一MetaStore；时间推进使用可控时钟；存储故障在真实事务边界注入，外部KMS等不安全边界才可用受控替身。禁止以mock调用次数验证结果。

## 步骤

1. 读取本任务oracle、实际调用方和相邻Rust测试，固定Go行为及失败点；引用前三项清单中本任务所有行，已有相同行为则先复现当前证据，避免重复实现。
2. 在独立测试中编写失败场景，通过现有公共入口访问真实服务；先运行失败命令并保存预期断言与退出码。
3. 实现本行为必要的最小代码与局部装配；复用现有领域API，不把proto、transport或etcd类型引入领域层。
4. 先运行 `cargo fmt --all`，确认没有改动其他任务文件，再运行通过命令和相邻回归；所有Cargo过程遵守槽位规则。
5. 使用任务4建立的只读oracle入口/真实已有Go二进制取得同场景差分，检查状态、错误和实际副作用；无运行条件就明确记录缺口，不把源码推导写成差分通过。
6. 在 `docs/rust/server-api/task-038.md` 记录基准、源码映射、失败/通过命令、退出码、有效测试数量、实际槽位及CARGO_TARGET_DIR，更新本任务范围的覆盖证据；仅测试通过后允许局部重构。

## 验证

- 聚焦：`cargo test --locked -p astersql-pd-server --lib task_038_ -- --nocapture`，必须至少1个实际行为测试运行并通过，不允许ignored或0 tests作为成功。
- 包级：`cargo test --locked -p astersql-pd-server -- --nocapture`；`cargo fmt --all -- --check`；`cargo check --locked -p astersql-pd-server`。
- Go只读守卫：开始/结束核对所有 `.go`、`go.mod`、`go.sum`、`go.work`、`go.work.sum` 的存在性及内容SHA-256和Git状态，包含untracked文件；保留用户原有差异，禁止新增或还原其他人的改动。
- 所需证据：预期行为失败与通过、真实成功/错误/取消案例、源码与测试路径、RPC/HTTP观察和持久状态、差分结果、退出码、有效测试数；进程/worker与锁均清理，缓存保留。当前源码若已覆盖行为，则补缺失验证并提交实际证据，不人为制造失败。

## 完成

该行为及本任务全部来源行获得当前证据才标记已完成；不得把扩大范围作为修改Go或绕过验证的授权。只能在本任务文件记阻塞；因无关环境无法验证可标记已完成，待回归并保留文件，但缺失领域能力或必要兼容证据须保持已阻塞。提交规则与删除规则见对应prompt；不修改plan.md或来源史诗。

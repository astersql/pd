# 任务 40: 归因wire来源覆盖与漂移

批次：【批次 40】依赖批次 36、37、38

状态：未开始

目的：对wire清单每一来源行和文件给出当前可验证的归属。

来源任务：原服务端任务102；当前证据 `docs/rust/server-api/task-102.md`；本任务来源为基准清单365个唯一路径、历史已发现20项变化只是起点；必须识别后续新增/删除及逐行为证据，不能用git log当兼容证明。

预计会话范围：只处理本文单一验收故事和必要局部接线；领域内核、后端选择及独立服务算法由列明的既有任务交付。发现已实现时补缺失验证，不另起平行实现。

## 执行顺序与依赖

- 默认线性遍历1至44，本任务为第40步；实际依赖批次：36、37、38。每个批次只有本任务，不能并行执行。
- 若有非依赖任务阻塞，可以在其记录阻塞、结束所有子进程并释放资源后执行本任务；不得跳过本任务自己的依赖或验证。

## Go 只读硬约束

- Go 仅供只读参考。禁止新增、修改、删除、重命名任何 `.go` 文件（含生产代码、测试、oracle、生成文件及临时目录中的 Go 工具）；`go.mod`、`go.sum`、`go.work`、`go.work.sum` 同样只读。所有新增实现与测试使用 Rust，必要的非 Go 验证脚本和证据文档须在任务范围内。
- 禁止通过生成器、格式化、依赖整理、failpoint 注入或构建脚本间接改写 Go；不得运行会改写 Go 的 make 测试入口、failpoint 启停命令、`go generate`、`go mod tidy` 或 Go 格式化写入命令。不得以“只增加测试/工具”“临时修改后还原”或扩大任务范围绕过约束。
- Go oracle 仅复用已有只读源码、测试、fixtures、已保存输出或已有二进制；运行已有 Go 工具/测试前，必须确认整个调用链不写上述文件且不需要 failpoint 注入。需要 failpoint 的测试不得直接裸跑来替代验证；无法只读取得必要运行证据时，记录具体缺口并按任务阻塞规则处理，不降低验收标准，不将源码推导冒充运行差分。
- 开始与结束时核对上述文件的 Git 状态及内容，保留用户已有变更；本任务不得产生任何 Go 文件或 Go 模块文件差异。本约束优先于计划、执行技能或验证要求中任何相冲突的写入指令。

## 文件

- 修改/扩展既有：`tests/rust-compat/workspace_acceptance.rs`。
- 修改/扩展既有：`tests/rust-compat/workspace_acceptance_test.rs`。
- 拟建：`docs/rust/server-api/unblock-102/wire-coverage.json`。
- 修改/扩展既有：`docs/rust/server-api/wire-inventory.md`。
- 测试注册：`tests/rust-compat/lib.rs`；仅实际需要依赖才修改所属Cargo.toml、根Cargo.toml/Cargo.lock，统一workspace.dependencies，不新建平行crate。
- 持久证据（拟建）：`docs/rust/server-api/unblock-102/task-040.md`，保存实际输入、版本、命令、失败/通过与有效数量。

## 上下文

- 只读参考：`docs/rust/server-api/wire-inventory.md`。
- 只读参考：`docs/rust/server-api/task-102-resume-evidence.json.gz`。
- 当前crate和测试入口以实际Cargo.toml为准；默认features，不增加推测性feature。所有命令从仓库根运行。
- 真实Go/Rust差分使用 `PD_COMPAT_MODE=dual`、绝对 `PD_COMPAT_GO_BIN` 与 `PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server"`；构建命令为 `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server`。
- 使用现成官方3.5.15并设置 `PD_COMPAT_ETCD_BIN`；需要etcdctl的supervisor同时设置 `PD_ETCD_BIN`，二者指向同一已验证二进制。协议ZIP用显式GOMODCACHE，PROTOC使用已有固定工具；工具链遵守仓库文件，替代版本必须单列证据，不能当同版本验证。
- 测试独立于生产模块并在所属lib.rs显式注册；新文件和下列新测试名前缀是拟建契约。先打通真实Cargo target，再取得行为断言失败；编译错误或0 tests不算预期失败。
- 历史76个指标family属性通过、maintenance差分、283个server测试不能传递为完整领域/HTTP通过。原始业务结果和指标属性结果分别计数。

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

- crate：`astersql-pd-compat-tests`；features：默认集合；新测试前缀：`unblock102_t040_`。
- 行为：基准清单365个唯一路径、历史已发现20项变化只是起点；必须识别后续新增/删除及逐行为证据，不能用git log当兼容证明。
- 失败验证测试：在 `tests/rust-compat/workspace_acceptance_test.rs` 新增 `unblock102_t040_behavior` 及对应边界/错误测试，先显式注册模块。
- 失败验证命令：`cargo test --locked -p astersql-pd-compat-tests --lib unblock102_t040_ -- --nocapture`。
- 预期失败原因：旧manifest哈希变化仅完成路径定位；尚未逐项关联当前行为、测试和后继证据。
- 通过验证命令：`cargo test --locked -p astersql-pd-compat-tests --lib unblock102_t040_ -- --nocapture`，必须至少1个真实测试通过且0失败、0跳过；修改Rust后先运行 `cargo fmt --all`。
- 模拟策略：真实HTTP/TCP/tonic、原有共享MetaStore与隔离etcd优先；事务故障在真实边界控制，时间用已有可控时钟。纯元数据负向控制可破坏临时输入，但不能以伪造正向产物或mock次数代替真实执行。

## 步骤

1. 只读解析实际格式：source为带反引号Markdown表、wire为sha256双空格路径块、surface为无反引号表；验证条数、唯一性和集合摘要，不允许解析零项通过。
2. 逐项输出旧/新SHA、来源ID/owner、负责后继任务、公开行为、当前测试命令、有效数量及证据摘要；同时盘点原manifest之后新增Rust/Cargo/脚本输入。
3. 对缺文件、重复行、遗漏映射、证据过期、把源码推导或指标-only报告当业务证明等添加独立负向测试。
4. 逐条处理本清单未解释变化；只有全部必要行有当前证据才完成，不能把支持范围外能力写成兼容通过。

## 验证

- 运行：`cargo test --locked -p astersql-pd-compat-tests --lib unblock102_t040_ -- --nocapture`。
- 运行：`cargo check --locked -p astersql-pd-compat-tests`。
- 运行：`cargo fmt --all -- --check`。
- 新增前缀测试和沿用的原回归分别统计；原用例如为ignored必须由列明supervisor显式实际执行，不能把默认忽略或过滤后的数量当通过。
- 所需证据：开始/结束Go路径、SHA和Git状态守卫（含ignored/untracked），保留用户既有改动；确切命令、退出码、有效测试数、实际响应/状态和清理结果。历史输出、源码推导与本轮运行分列。
- 业务变更保留预期行为失败与通过；已有行为确证已覆盖时不人为造失败，补当前可复现证明。元数据/产物任务的正向验证必须指向真实产物，缺失时不能谎称完成。
- 不修改其他任务状态，不写旧plan.md/prompt.md或来源史诗；本计划最终任务44才可按原规则处理旧102。

## 完成

只有本文单一行为与全部前置证据满足才标记 `已完成`。必要领域或兼容证据缺失必须 `已阻塞`；仅无关环境阻止已实现行为回归时才可 `已完成，待回归`，不能用于绕过oracle、领域或durability门槛。有实际代码/测试/配置/文档变更时使用 `$git-commit` 检查并提交本任务变更，排除其他任务及用户改动；只有编号文件状态/删除则不提交。完成证据在最终回复报告后按执行技能删除本编号文件；阻塞或待回归保留文件。不要修改plan.md或来源史诗。

# 任务 1: 准备Go基准与工具

批次：【批次 1】无依赖

状态：未开始

目的：准备当前回归真正缺少的输入，不再拆成四个产物任务。

来源任务：原服务端任务102及docs/rust/server-api/task-102.md末尾剩余记录。

预计会话范围：只处理这项收尾行为；缺外部产物就记录明确阻塞，不扩展为新一轮领域开发。

## 执行顺序与依赖

- 线性顺序1→6；本任务为第1步，无依赖。批次仅本任务，不并行。
- 既有GC/keyspace/MCS/生产后端任务不属于本轮执行范围；只记录其当前交接状态，不复制、重拆或假定完成。

## Go 只读硬约束

- Go 仅供只读参考。禁止新增、修改、删除、重命名任何 `.go` 文件（含生产代码、测试、oracle、生成文件及临时目录中的 Go 工具）；`go.mod`、`go.sum`、`go.work`、`go.work.sum` 同样只读。所有新增实现与测试使用 Rust，必要的非 Go 验证脚本和证据文档须在任务范围内。
- 禁止通过生成器、格式化、依赖整理、failpoint 注入或构建脚本间接改写 Go；不得运行会改写 Go 的 make 测试入口、failpoint 启停命令、`go generate`、`go mod tidy` 或 Go 格式化写入命令。不得以“只增加测试/工具”“临时修改后还原”或扩大任务范围绕过约束。
- Go oracle 仅复用已有只读源码、测试、fixtures、已保存输出或已有二进制；运行已有 Go 工具/测试前，必须确认整个调用链不写上述文件且不需要 failpoint 注入。需要 failpoint 的测试不得直接裸跑来替代验证；无法只读取得必要运行证据时，记录具体缺口并按任务阻塞规则处理，不降低验收标准，不将源码推导冒充运行差分。
- 开始与结束时核对上述文件的 Git 状态及内容，保留用户已有变更；本任务不得产生任何 Go 文件或 Go 模块文件差异。本约束优先于计划、执行技能或验证要求中任何相冲突的写入指令。

## 文件

- 扩展既有：`scripts/pd-compat-readonly.py`。
- 扩展既有：`tests/rust-compat/readonly_oracle.rs`。
- 扩展既有：`tests/rust-compat/readonly_oracle_test.rs`。
- 扩展既有：`tests/rust-compat/go_client_acceptance_test.rs`。
- 证据写入既有 `docs/rust/server-api/task-102.md` 及其压缩记录；只追加本轮数据，不覆盖历史失败。

## 上下文

- 默认features，所有命令从仓库根运行；先读AGENTS.md及docs/rust/architecture.md、docs/rust/runtime.md。
- 使用绝对PD_COMPAT_GO_BIN、PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server"及PD_COMPAT_MODE=dual。native fixture使用PD_COMPAT_ETCD_BIN=官方3.5.15绝对路径；HTTP/member验证同时设置PD_ETCD_BIN，旁边须有etcdctl。Go build info检查工具、固定协议ZIP与PROTOC均使用已有已核对输入。
- 当前公开factory为PdNodeBuilder::start、PdServices和现有harness；测试使用真实TCP/tonic/HTTP和共享MetaStore，不新建平行服务。

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

- crate：`astersql-pd-compat-tests`；features：默认集合。验证入口沿用下面现有用例或supervisor，不为每个验证另建任务和空壳测试。
- 已知失败：现有Go发布二进制使用107095协议且丢字段；三个工具缺失，不能跳过task099。
- 新增负向测试放在现有readonly_oracle_test.rs，名称前缀 `tail102_inputs_`（拟建），在现有测试模块中注册；失败/通过命令：`cargo test --locked -p astersql-pd-compat-tests --lib tail102_inputs_ -- --nocapture`。正向还必须实际验证真实基准和工具，不能只验证伪造元数据。
- 模拟策略：真实二进制、真实etcd及实际事务边界；纯元数据负向控制可破坏临时数据，但不能用mock次数或静态返回代替业务。

## 步骤

1. 优先查验已有或可信发布产物，记录URL/原位置、SHA、Go build info、协议、架构和Dashboard构建/运行配置。
2. 若选择构建现有Go程序，先证明完整SDK和全部模块源码缓存已存在，整条调用链及临时目录不产生任何Go文件；使用GOTOOLCHAIN=local、GOPROXY=off、GOWORK=off及-mod=readonly，只把可执行文件输出到所领槽位。不能解压Go模块或SDK源码补缺，不能运行生成器或make写Go。
3. 在已有readonly_oracle_test.rs补缺文件、旧协议和错误产物的失败校验；正向使用真实二进制通过当前字段回读用例，不伪造版本字符串。
4. 设置PD_COMPAT_MODE=dual及绝对Go/Rust路径，供后续复用；缺合格输入如实记录已尝试来源，不把旧Go窄合同证明升级为完整协议验收。

## 验证

- 运行：`cargo test --locked -p astersql-pd-compat-tests --lib readonly_oracle_test -- --nocapture`。
- 判据：
  - Go PD消费固定kvproto f8ad079c8e9812ed5900b0c12b5f1d97171464c4，并实际保留txn_protocol_version_range。
  - 所领槽位根存在真实pd-region-bench、pd-tso-bench、pd-heartbeat-bench，来源、版本、架构、权限和CLI入口可核对。
- 记录退出码、有效测试数、原始响应/状态、允许的归一化与副作用。0 tests、默认ignored或指标属性单独通过不得计作完整业务成功。
- 开始/结束核对全部Go/模块文件的存在性、SHA和Git状态（含ignored/untracked）；清理自己的worker与进程后再释放锁，保留缓存和用户原有改动。

## 完成

只有本文范围获得当前证据才标记已完成；缺必要oracle、实现或兼容证据应保持已阻塞，不能以已完成，待回归规避。仅无关环境导致已实现行为无法验证时可标记已完成，待回归并保留文件。有实际改动用$git-commit提交本任务，排除无关变更；只有编号文件状态或删除不单独提交。完成证据在最终回复报告后按执行技能删除本编号文件；阻塞或待回归保留。plan.md、已有领域计划及来源史诗只读。

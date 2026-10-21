# 任务 8: 装配最小 PD 运行主链

批次：【批次 8】 依赖批次 7

状态：未开始

目的：把 member、ID、TSO、MetaStore 和协议适配器装配为第一个可运行 Rust PD 节点，同时保留独立与可嵌入两种启动方式。

来源任务：无

预计会话范围：仅接入健康、成员、ID 测试入口和 TSO gRPC；不实现 region heartbeat、调度、Dashboard、keyspace group 或 MCS。

## 文件

- 新建：`pkg/server/Cargo.toml`
- 新建：`pkg/server/lib.rs`
- 新建：`pkg/server/grpc.rs`
- 新建：`pkg/server/http.rs`
- 新建：`pkg/server/server_test.rs`
- 修改：`pkg/runtime/node.rs`
- 修改：`pkg/runtime/node_test.rs`
- 修改：`cmd/pd-server/main.rs`
- 修改：`cmd/pd-server/main_test.rs`
- 修改：`tests/rust-compat/health_member_test.rs`
- 新建：`tests/rust-compat/tso_test.rs`
- 修改：`Cargo.toml`

## 上下文

- 网络层接收领域服务 trait，负责 protobuf/HTTP 转换、状态码、header 和指标；领域服务不依赖 tonic/axum 类型。
- gRPC 与 HTTP 需要在兼容端口上共存；独立二进制创建 Tokio runtime 和信号，嵌入路径由调用方传入 runtime、listener、取消令牌和指标注册表。
- `ready` 只能在 MetaStore、leadership 和所有 listener 可服务后置位；shutdown 按接入、后台任务、leadership、storage 的逆序进行。

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

- 行为：真实客户端可通过同一 listener 访问 HTTP health/member 和 gRPC TSO；独立与嵌入启动路径拥有相同 ready/shutdown 顺序。
- 失败验证测试：先扩展 runtime 测试和兼容测试台，要求真实网络访问上述端点。
- 失败验证命令：`cargo test -p astersql-pd-server -p astersql-pd-runtime -p astersql-pd-compat-tests -- --nocapture`
- 预期失败原因：server crate 和正式装配接线尚不存在。
- 通过验证命令：`cargo test -p astersql-pd-server -p astersql-pd-runtime -p astersql-pd-cmd-pd-server -p astersql-pd-compat-tests -- --nocapture`
- 模拟策略：只注入端口和测试时钟；网络、协议、runtime、EtcdMetaStore 和进程生命周期走真实路径。

## 步骤

1. 为同端口路由、ready 顺序和双启动模式编写失败测试。
2. 实现 gRPC/HTTP adapters 及统一 listener。
3. 在 `PdNodeBuilder` 装配 member、ID 和 TSO。
4. 更新独立二进制入口，保持其为薄包装。
5. 在双实现测试台执行 health/member/TSO 差分。

## 验证

- 运行：`cargo test -p astersql-pd-server -p astersql-pd-runtime -p astersql-pd-cmd-pd-server -- --nocapture`
- 运行：`./scripts/run-pd-compat.sh --mode dual --case health-member --case tso`
- 预期：独立和嵌入路径都通过；差分无未批准字段；进程退出后无后台任务、端口或临时目录残留。
- 所需证据：实际槽位、命令退出码、测试数量、差分摘要以及两条生命周期事件序列。

## 完成

固定响应、仅内存状态或只通过 mock 客户端的 server 不算完成。未实现 RPC 必须返回明确 unsupported，而不是成功空响应。


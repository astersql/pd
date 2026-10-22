# 任务 7: 补齐 PD 服务端与 API

批次：【批次 7】依赖批次 6

状态：未开始

目的：扩展现有 Rust server/runtime，接入完整集群与调度能力，并实现兼容的 PD gRPC、HTTP、配置、TLS、指标和进程生命周期。

来源任务：对应本主题计划的主题 7；扩展现有 `astersql-pd-server`、`astersql-pd-runtime` 和 `pd-server`。

预计会话范围：这是服务装配史诗。必须按 cluster worker、gRPC 方法族、HTTP API 组、中间件、join/bootstrap、CLI/runtime 分别创建子计划。

## 文件

- 修改：`pkg/server/`
- 修改：`pkg/runtime/`
- 覆盖差距：`pkg/proto/`
- 覆盖差距：`pkg/tso/`
- 修改：`cmd/pd-server/`
- 迁移源：`server/cluster/`
- 迁移源：`server/api/`
- 迁移源：`server/apiv2/`
- 迁移源：`server/join/`
- 迁移源：`server/server.go`
- 迁移源：`server/grpc_service.go`
- 迁移源：`cmd/pd-server/main.go`
- 迁移源：`pkg/basicserver/`
- 迁移源：`pkg/utils/apiutil/`
- 迁移源：`pkg/utils/grpcutil/`
- 迁移源：`pkg/utils/logutil/`
- 迁移源：`pkg/utils/memberutil/`
- 迁移源：`pkg/utils/metricutil/`
- 迁移源：`pkg/utils/netutil/`
- 迁移源：`pkg/utils/requestutil/`
- 迁移源：`pkg/utils/tempurl/`
- 测试：`tests/server/`、`tests/rust-compat/` 和新增 Rust 独立测试

## 上下文

- 现有 Rust proto、TSO 和 server 只覆盖最小纵切；必须先做协议、领域能力、方法和 API 覆盖差距清单，扩展现有 crate 而不是新建平行实现。
- runtime 继续接受宿主 Tokio handle、取消令牌、Registry 和 listener；库不得安装信号、退出进程或创建私有 runtime。
- gRPC/HTTP 同监听器、Go client 兼容、TLS、认证、限流、审计、错误码与 metrics 都属于外部契约。

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

- 测试驱动开发说明：每个 RPC/API/生命周期子计划先补 Go/Rust 双实现失败场景；主题层验证 Go client、同端口路由、滚动重启和关闭。
- Go oracle：`make gotest GOTEST_ARGS='./server/... ./cmd/pd-server ./pkg/basicserver ./pkg/utils/apiutil/... ./pkg/utils/grpcutil ./pkg/utils/logutil ./pkg/utils/memberutil ./pkg/utils/metricutil ./pkg/utils/netutil ./pkg/utils/requestutil ./pkg/utils/tempurl -count=1'`
- Rust server：`cargo test --locked -p astersql-pd-server -p astersql-pd-runtime -p astersql-pd-cmd-pd-server -- --nocapture`
- 兼容回归：`cargo test --locked -p astersql-pd-compat-tests -- --nocapture`
- 静态检查：`cargo fmt --all -- --check && cargo check --workspace --locked`
- 模拟策略：使用真实 TCP listener、Go/Rust 进程和客户端；仅外部 KMS 等不安全边界允许受控替身。

## 步骤

1. 生成 gRPC 方法、HTTP 路径、配置项、指标和 CLI flag 的覆盖差距清单并分组建计划。
2. 先接入 cluster worker 与核心 gRPC，再迁移 HTTP API、中间件和 join/bootstrap。
3. 补齐 TLS、认证、限流、审计、metrics 与错误映射。
4. 完成独立进程和嵌入模式的启动、ready、失败清理、滚动重启与 shutdown 验证。

## 验证

- 运行：上述 Go oracle、Rust server、兼容回归和静态检查命令。
- 预期：全部退出码为 0；Go client 可运行核心业务；API/指标/配置覆盖清单无未解释缺口；关闭后无后台任务持有存储或 listener。
- 所需证据：覆盖矩阵、协议/API 差分、真实进程测试、滚动重启/故障结果、命令退出码和有效测试数。

## 完成

服务端外部契约和生命周期通过主题验收后标记 `已完成`；提交实际变更并删除本任务文件。Dashboard、工具和原生元数据后端仍由后续任务处理。

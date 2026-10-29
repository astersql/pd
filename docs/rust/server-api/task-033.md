# 任务033：全局配置 Watch

状态：已完成。基准 `4f57a12fa`；依赖批次32的读写实现及任务1–4的清单、准入、只读兼容入口已核对。固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认 features，无 Cargo 依赖变更。

覆盖 [source-inventory.md](source-inventory.md) 的任务33行及 grpc_service.go 公开方法来源行、[wire-inventory.md](wire-inventory.md) 的 WatchGlobalConfig 行。[surface-inventory.md](surface-inventory.md) 的限流、审计、指标仍由79–81/90–92验收；本方法没有新增 HTTP 管理路由。

只读 oracle：[server/grpc_service.go](../../../server/grpc_service.go) 的2550–2630行、[grpcutil/cluster.go](../../../pkg/utils/grpcutil/cluster.go) 的 ErrorHeader、[client/client.go](../../../client/client.go) 的 WatchGlobalConfig 及已有 [Go集成测试](../../../tests/integrations/client/global_config_test.go)。没有运行需要 failpoint 的 Go 测试，没有新增或修改 Go 工具。

[适配实现](../../../pkg/server/grpc_global_watch.rs) 复用任务32的路径归一化、宿主 discovery_store 和取消令牌；Grpc 与 SyncGrpc 注册同一方法。请求不依赖 raft bootstrap 或请求 header。响应流直接拥有 backend watch，不创建独立生产者、无界队列或私有 runtime，客户端断开和宿主关闭会 drop 正在等待的后端 RPC。revision>0 包含指定 revision；revision<=0 从当前快照 revision+1 开始。成功响应只有 Changes 和 Revision；PUT/DELETE 均使用 Payload 字节，DELETE 返回前一版本，前一版本被压缩时跳过该事件。暂时不可用时复用 MetaWatch 的恢复游标并执行可取消的100ms退避。

[MetaStore契约](../../../pkg/storage/kv/meta_store.rs) 添加显式 watch_with_previous 和最近批次 previous_value；[etcd适配](../../../pkg/storage/kv/etcd_store.rs) 仅在选择该模式时请求 PrevKV，保留按事件 revision/key 索引的旧值。普通 watch 保持原选项。[MemoryStore](../../../pkg/storage/kv/memory_store.rs) 保留批次旧值，并修正压缩边界为保留边界 revision 的事件；[跨后端契约](../../../pkg/storage/kv/contract_test.rs) 对真实 etcd 和内存模型核对边界回放、二进制旧值和返回批次在后续 compact 后仍持有旧值。存储领域未引入 proto、tonic 或 etcd 类型。

[真实TCP测试](../../../pkg/server/grpc_global_watch_test.rs) 验证未 bootstrap 的历史回放、同端口 HTTP ping、路径拒绝、缺后端错误优先级、持久字节/删除副作用。故障测试通过已有 fixture 的 stop/start 关闭并重启真实 etcd，核对断线恢复后的 revision 和 payload；观察真实 backend watch 的资源持有数，验证客户端断开、deadline、宿主 shutdown 后归零。该计数用于资源释放断言，不以调用次数替代结果。完整 PdNode 会在后端停机时因成员角色循环结束而停止宿主，因此重连故障测试使用公开 router 的独立嵌入宿主和同一真实 MetaStore；完整节点另测正常路径，不声称修改了成员生命周期策略。可控 Tokio 时钟测试推进一小时，验证整个流的 deadline 终止。真实 TCP 测试另验证远端 grpc-timeout；服务端同时约束建立阶段与响应流，超时不保留 backend watch。

[共享场景](../../../pkg/server/grpc_global_watch_scenarios_test.rs) 经[运行测试](../../../pkg/runtime/grpc_global_watch_differential_test.rs)和[只读supervisor](../../../scripts/pd-compat-readonly.py)对真实已有 Go PD 二进制运行。场景比较二进制 PUT/DELETE 的完整 protobuf item 字节、历史顺序、断开后从下一 revision 重新订阅、事务内多 key 顺序、指定 prefix、路径状态/消息、真实 compaction 的响应及 trailers、边界 revision 回放、旧值已压缩的删除跳过及后续 PUT、流 deadline 和持久状态。

差分只归一化已逐端断言的绝对 revision、集群 ID 和压缩消息中的对应数字；不归一化业务字节、事件 kind、错误种类或压缩 terminal status。Go 使用真实集群 ID 构造错误 header；压缩错误先发 DATA_COMPACTED，末尾为 Unknown/`etcdserver: mvcc: required revision has been compacted`。边界 revision 本身可回放。deadline 存在远端 gRPC 终止与 HTTP/2 RST_STREAM CANCEL 的竞态：严格接受 DeadlineExceeded/Cancelled 及已确认的三种消息（context deadline exceeded、context canceled、h2 protocol error: error reading a body from connection），保留原始诊断，只比较终止取消语义。错误或忽略测试不计通过。真实 Go 后端内部断线未被注入；Go 差分覆盖客户端断开后重新订阅，后端停机/恢复由 Rust 的真实 etcd 故障测试覆盖。

初始测试可编译运行，1项在 Unimplemented 行为断言失败（退出101）。中间证据还保留：接口导入及序列化编译错误、完整节点随成员循环结束导致 EOF、缺少流 deadline 支持、压缩 terminal status、误判等于边界的行为、HTTP/2取消诊断、错误 header 缺集群 ID，以及显式旧值选项接线期间的存储测试失败；这些日志均不计为通过。

## 验证记录

从1–10按原子 mkdir 动态领取，实际均为槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。每次记录计划、任务、会话及持有进程 PID；所有 Cargo/fixture/supervisor 子进程继承该绝对路径，不使用 --target-dir。阶段结束释放自己的记录与锁，缓存保留。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_033_ -- --nocapture` | 101 | 1行为断言失败，非编译错误 |
| 最终同一聚焦命令 | 0 | 4通过，0忽略 |
| `cargo test --locked -p astersql-pd-storage-kv -- --nocapture` | 0 | 44通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 144通过，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case global-watch` | 0 | 1真实差分通过，0忽略 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 2017通过（含2文档测试），28忽略，不计通过 |

最终监督报告 `compat-readonly-1793279607075499000.json`：exit=0；readonly_unchanged、inputs_unchanged、work_directory_cleaned、process_group_cleaned 均为 true。其余7次失败尝试的四项守卫也全部为 true，失败不计通过。所有生产、测试代码在最终聚焦、包级、差分、workspace 检查/测试前已固定；此后只更新证据文档。

已有 Go oracle 为 go1.26.8 构建的真实 PD 二进制，kvproto 模块为 `v0.0.0-20261016101550-f8ad079c8e98`；SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。二进制没有 VCS revision，不声称来自当前 HEAD；完整 build info 与每次二进制/源码输入哈希保存在监督报告中。

开始/结束含 ignored/untracked 和 target 的1983个 Go/模块文件，存在性、每文件 SHA-256 及受保护 Git 状态完全相同。plan.md 与开始基准逐字节一致。没有执行 Go 测试、make、failpoint 启停或 Go 格式化；仅复用已验证的只读 Go fixture 构建链与已有 oracle。自己的锁和 PD/fixture 进程均已清理，缓存保留。原有编号任务1–32的删除不纳入本任务提交；任务33按要求在完成后删除。

Python AST、入口 bash 语法、文档链接和 git diff --check 通过；提交前另核对暂存集合及 diff。原始命令、退出码、测试摘要、Go首尾清单、8份监督报告、完整输出、源码哈希与槽位执行器见[压缩证据](task-033-evidence.json.gz)，SHA-256：`b33824d3a556a0dd03a98b008b8a61879772c89eb6fd63ed07cf7164a33cad23`。

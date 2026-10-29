# 任务027：Split 结果上报

状态：已完成。基准 `5bb6c7419`；已核验任务1–4、三份清单及依赖任务26证据。协议仍固定 kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认 features。

## 契约与来源

覆盖 [source-inventory.md](source-inventory.md) 的任务27、cluster_worker.go、grpc_service.go 来源行，以及 [wire-inventory.md](wire-inventory.md) 的 ReportSplit/ReportBatchSplit 两行；[surface-inventory.md](surface-inventory.md) 没有独立 owner=27 行。

只读 Go 映射：`server/grpc_service.go:1841–1907`、`server/cluster/cluster_worker.go:196–275`。任务目的中“校验父子范围和epoch后应用状态变化”与实际 Go 不同：Go 只检查相邻 end/start key 相等，且右 end 为空或左 start 小于右 end；单次要求两侧存在，批量要求至少两个 region。它不检查 epoch、ID、父区间、peer 或当前集群成员关系，也不更新 region 缓存/持久状态。真实 Go 差分确认缺失 epoch、较新 epoch、未知/重复 ID 均可接受；无界右端保留 Go 的特殊接受行为。按 oracle 实现，不擅自强化校验或把上报当心跳。

[grpc_report_split.rs](../../../pkg/server/grpc_report_split.rs) 复用既有角色、集群身份、bootstrap 与取消入口；[grpc.rs](../../../pkg/server/grpc.rs) 和 [sync_transport.rs](../../../pkg/server/sync_transport.rs) 装配两个 RPC。错误为 UNKNOWN header，消息 `invalid split region`；成功保留 cluster ID。没有新领域模型、依赖、Cargo.lock 或 MetaStore 写事务；持久化故障注入不适用于这个只读上报操作。

## 当前行为证据

[独立行为测试](../../../pkg/server/grpc_report_split_test.rs) 通过真实 etcd、TCP/tonic 及同监听器 HTTP ping，验证两种未 bootstrap 响应、成功/非法范围、内存 GetRegionByID 与持久 region entries 不变，以及取消后请求终止并 shutdown/join。

[共享场景](../../../pkg/server/grpc_report_split_scenarios_test.rs) 包括缺失两侧/一侧、相邻范围、较新及缺失 epoch、gap、倒序、有界/无界、空/单元素/多元素 batch、错误 cluster ID。返回15个完整 protobuf header，只在核实 cluster ID 后将该随机字段归零；错误类型和文本不归一化。双方上报前后均比较同一 region 查询的完整响应。

[差分测试](../../../pkg/runtime/grpc_report_split_differential_test.rs) 通过任务4只读监督入口，对 Rust 与既有 Go leader 比较上述15组响应，并比较两种 RPC 在 follower 的 Unavailable 状态和错误文本。Rust follower 和 Go join follower 都是真实服务。Go oracle 为既有 `bin/pd-server`，go1.26.8，build info 无 VCS revision，不能声称对应当前 HEAD。其哈希、build info、监督报告及全部原始日志见压缩证据。

初次聚焦测试可编译运行，真实 TCP 返回 Unimplemented，行为断言失败（101），不是编译错误。随后实现后聚焦通过。首轮差分因测试误将 follower 的 Unavailable 写成 Unknown 而失败（101）；按已有角色契约修正测试后真实差分通过，未修改生产角色门。

## 命令与验收

所有 Cargo 阶段从根目录依次原子 mkdir 领取1–10共享槽位，记录任务/会话/PID；使用槽位1与2，绝对 `CARGO_TARGET_DIR` 分别为 `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1` 和 `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。生产实现、聚焦、包级及最终差分使用1；并发构建/静态及 workspace 回归使用2。子进程继承目录，无 --target-dir；阶段结束释放自己的锁，保留缓存。Rust 修改后执行 cargo fmt --all。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初次 `cargo test --locked -p astersql-pd-server --lib task_027_ -- --nocapture` | 101 | 1行为失败 |
| 最终同一聚焦命令 | 0 | 1通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 132通过，0忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case report-split` | 0 | 1真实差分通过，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |

最终只读监督报告为 `compat-readonly-1793267651668477000.json`，exit=0，readonly_unchanged、inputs_unchanged、work_directory_cleaned、process_group_cleaned 全为 true。完整报告包括1983个 Go/模块文件（含 ignored/untracked/target 缓存）。任务最初快照脚本漏扫 target，仅记录1101文件；随后逐字节验证完整1983集合与依赖任务26结束快照相同，并由本次监督入口取得完整运行前后快照。这里明确区分初始直接测量与前序证据，不把补核伪称初始全量扫描。非 target 初始集合及 Git 状态亦在结束时复核；没有创建或改写任何 Go/模块文件，没有运行 make、failpoint 或 Go 测试。

plan.md 保持只读，既有任务1–26删除保持原样且不提交。只暂存本任务 Rust、局部接线、监督入口及证据；最终按要求删除任务27文件。

`cargo test --workspace --locked -- --test-threads=1`：退出0，1994通过（1992常规测试、2文档编译失败示例），0失败，22忽略且不计通过；本任务差分已单独显式运行。最终 Python AST、文档相对链接、git diff --check、plan.md 与 HEAD 字节比较通过。全部 Cargo/测试服务进程结束，自有槽位锁已释放。

[压缩证据](task-027-evidence.json.gz) 包含初始失败、通过、回归、差分原始日志与确切命令/退出码/槽位/测试摘要，源码哈希、Go oracle provenance、完整只读监督报告及快照。SHA-256：`3f67b643df5be16ffe5496813366929efeccdefb2a2fb8722a62f8cf09b56608`。最终完整1983文件集与监督入口运行前后快照一致；初始1101文件集和Git状态不变。

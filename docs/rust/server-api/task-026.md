# 任务026：Split 标识申请

状态：已完成。基准 `bce44ca7b`；批次25证据及任务1–4、三份清单已核验。协议沿用固定 kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`，默认 features。

## 来源与实现

覆盖 [source-inventory.md](source-inventory.md) 的任务26行和 grpc_service.go 来源行、[wire-inventory.md](wire-inventory.md) 的 classic PD AskSplit/AskBatchSplit 两行；[surface-inventory.md](surface-inventory.md) 没有独立 owner=26 行。只读 Go 映射为 `server/grpc_service.go:1734–1838`、`server/cluster/cluster_worker.go:77–198`、`pkg/core/region.go:2372–2390`、`pkg/id/`，版本依赖补充映射 `server/cluster/cluster.go:2360–2395`、`pkg/versioninfo/feature.go:58`。独立 schedulingpb 服务和其它调度行为不计作本项验收。

[grpc_ask_split.rs](../../../pkg/server/grpc_ask_split.rs) 经现有 [Grpc](../../../pkg/server/grpc.rs)/[SyncGrpc](../../../pkg/server/sync_transport.rs) 接入两个 RPC。通过 `PdServices → RoleServices/Application → ClusterHost` 调用 [cluster_split.rs](../../../pkg/runtime/cluster_split.rs)，在同一资源锁内复用 canonical `RegionsInfo::valid_region` 与当前任期的 fenced ID allocator；没有复制 region 校验算法、引入平行缓存或改变领域依赖方向。协议类型止于 server 适配层。

按起始键查找当前 region，同时比较 version/conf_ver，拒绝任一字段落后的请求；缺失 epoch 按零值校验，较新 epoch 可接受。ID/end_key 不用于替代起始键校验；peer 数量来自请求，每个结果按 region ID、peer IDs 的顺序逐个申请，复用全局分配器。零 split_count 仍执行校验；错误不返回部分结果，已消费的 ID 不回滚。

AskSplit 先角色校验再 bootstrap；AskBatchSplit 先 bootstrap 再角色校验，与 Go 保持不同的检查顺序。错误 header 保留 cluster ID、类型与消息。nil epoch 文本和 rich region 的十六进制 key、learner/witness、flashback 文本通过实际差分；保留既有 Go gogo 文本输出 `LearnerLearner`，不将它改成不同的错误消息。

差分复核发现旧 Rust 缺少 BatchSplit 的集群版本门，因此补齐必要依赖：注册 store 后从 canonical stores 计算最低版本，更新并持久化现有 `PersistedConfig.cluster_version`；持久化错误记入已有 host diagnostics。BatchSplit 在 region 校验前检查 `2.1.0-rc.1` 门槛。未注册时的版本门响应与注册后的成功路径均执行真实 Go 差分；[split_version_test.rs](../../../pkg/runtime/split_version_test.rs) 单独覆盖 rc.0/rc.1/rc.2/rc.10、稳定版本及 build metadata 的 precedence。

## 行为与差分

[server 行为测试](../../../pkg/server/grpc_ask_split_test.rs) 使用真实 etcd/TCP/tonic，同监听器 HTTP ping；覆盖未 bootstrap、版本门、两种 RPC 的成功/错误、follower 顺序、持久 region entries 不变、重启后新 ID 超过持久预留水位。

[共享场景](../../../pkg/server/grpc_ask_split_scenarios_test.rs) 覆盖 missing region、未覆盖 key、rich metadata、两个 epoch 字段落后、零/缺失 epoch、相同及较新 epoch、不同请求 ID/end_key、零 peers、零 count、错误 cluster ID、并发批量申请。双方各验证148个新 ID 唯一；错误请求不消费分配器 ID。

[runtime 故障测试](../../../pkg/runtime/grpc_ask_split_fault_test.rs) 在真实分配事务边界加入超过 etcd 限制的 mutation，验证事务整体失败、没有返回 split IDs、水位未变且 oversized key 不存在；重试成功。先耗尽已被协调器预留的窗口，避免把缓存分配当作事务故障验证。下一预留事务暂停时取消 host，通过真实 TCP 观察请求终止，关闭并等待节点，检查持久水位没有推进。

[差分测试](../../../pkg/runtime/grpc_ask_split_differential_test.rs) 由任务4只读监督入口显式执行；真实 Go leader/join follower 与 Rust 双节点执行同一场景。19组 leader 完整 protobuf 响应比较（含版本门），另比较 follower 的状态/错误消息和 batch header。仅归一化已断言正确的 cluster ID 与已验证非零、全局唯一的新 ID；没有归一化错误文本。存储事务故障、host 取消、重启水位和 region 持久状态为 Rust 专项证据，不冒充 Go 故障差分。依赖修复另执行已有 Store 注册 Go 差分。

Go oracle 为既有 `bin/pd-server`，build info 为 go1.26.8、无 VCS revision，不声称来自当前 HEAD；完整 build info、二进制哈希及只读监督报告保留在压缩证据。

## 验证记录

最终命令、退出码、有效测试数、源码哈希和只读守卫见下列完成记录与压缩证据。初始行为测试已编译并通过真实 TCP 调用，因 AskSplit 为 Unimplemented 而断言失败（退出101）；不是编译错误。中间测试修复和失败输出均保留，不能计作最终通过。

所有阶段均从仓库根依次原子 `mkdir target/rust-slot-locks/slot-N.lock` 动态领取1–10槽位；实际均取得槽位1，设置 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，所有子 Cargo/监督入口继承，无 `--target-dir`。每阶段记录任务/会话/PID，结束后删除自己的 owner 和锁，缓存保留。Rust 修改后先执行 `cargo fmt --all`。

| 确切命令 | 最终退出码 | 有效测试 |
| --- | --- | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_026_ -- --nocapture` | 101 | 1行为失败，非编译错误 |
| 最终同一 server 聚焦命令 | 0 | 1通过、0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_026_ -- --nocapture` | 0 | 2通过、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 131通过、0忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建，不计行为测试 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case ask-split --case store-registration` | 0 | 两个场景各1通过、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 1993通过（1991常规测试、2文档编译失败示例），21忽略且不计通过 |

中间失败包括差分入口重复传入 nocapture、重启测试改变已登记 listener 身份、故障测试没有耗尽预留窗口，以及错误 header/learner 文本的真实差异，均修正后重跑。一次较早 workspace 回归的测试目标运行后，在其完全结束前编辑了版本门源码，最终 doc-test 混合新源码和旧依赖而失败（退出1）；该次不算完整通过，保持源码冻结后重新执行全部验证。原始日志全部归档，不将中间通过数冒充最终结果。

最终只读监督报告 `compat-readonly-1793266532692703000.json`：readonly_unchanged、inputs_unchanged、work_directory_cleaned 与两个场景的 process_group_cleaned 均为 true。开始/结束保护集合1983个 Go/模块文件，含 ignored/untracked，路径、SHA-256和Git状态完全一致；plan.md与基准HEAD字节一致。没有修改 Go、模块文件、Cargo.lock 或依赖。最终进程检查未发现本任务测试/服务/fixture进程，自己的锁与 owner 已释放，缓存保留。Python AST、入口 bash 语法、文档相对链接和 git diff --check 通过。

[压缩证据](task-026-evidence.json.gz) 包含全部原始失败/通过日志、确切命令/退出码/有效测试摘要、去重的完整Go快照、源码哈希、二进制 provenance、只读监督报告和槽位执行器。SHA-256：`a901b0f9e1f3362f8411c619504fb9a23240e445643e765d8ae9c3839aba694b`。

原有任务1–25文件删除保持原样，不纳入本任务提交；任务26按要求标记完成后删除，不将任务状态变化混入实现提交。

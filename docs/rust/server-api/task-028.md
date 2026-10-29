# 任务028：集群配置 RPC

状态：已完成。基准 `c08fa8536`，依赖任务27已核验完成；复核任务1–4、source-inventory、wire-inventory 与 surface-inventory。保持默认 features 和固定 kvproto commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 契约与实现

覆盖 [source-inventory.md](source-inventory.md) 任务28及 grpc_service.go 来源行、[wire-inventory.md](wire-inventory.md) GetClusterConfig/PutClusterConfig 两行。[surface-inventory.md](surface-inventory.md) 的配置 owner 为47–50/90，没有本任务独立行。

只读 Go oracle：`server/grpc_service.go:1909–1972`、`server/cluster/cluster.go:1493–1501,2400–2415`。两个 RPC 操作 metapb.Cluster（id/max_peer_count），没有版本字段或版本校验；不是 server/config 的语义版本设置。Go 允许0及任意 uint32 副本数；配置 ID 必须等于当前集群 ID。缺失配置与错误 ID 返回 UNKNOWN header，错误文本保留 Go protobuf String 格式。未 bootstrap 返回 NOT_BOOTSTRAPPED；角色及 request header 校验复用现有入口。

[grpc_cluster_config.rs](../../../pkg/server/grpc_cluster_config.rs) 复用 StorageEndpoint 的 load_meta/save_meta、ClusterPaths 和已有 Cluster 类型。读写真实 MetaStore，不创建领域平行实现。runtime 的 RoleServices 通过已有 TermStore 提供领导任期围栏，写事务带 leadership 条件；grpc.rs 与 sync_transport.rs 两层均接线。没有依赖、Cargo.lock 或 Go 修改。

## 行为与差分

[独立测试](../../../pkg/server/grpc_cluster_config_test.rs) 通过真实 TCP/tonic、etcd 和同监听器 HTTP ping 验证：未 bootstrap 的两种响应、四种副本数、缺失配置、错误配置 ID、错误 request cluster ID、持久 protobuf 状态与服务完整关闭再启动恢复。重启复用同一监听地址和身份。

另一测试在真实事务边界给 etcd 事务添加超大写入，确认事务失败且旧配置和持久状态不变；阻塞配置事务后取消服务，确认请求结束、shutdown/join 完成且未写入新配置。注入仅门控真实 IO，不以 mock 调用次数断言。

[共享场景](../../../pkg/server/grpc_cluster_config_scenarios_test.rs) 与 [差分入口](../../../pkg/runtime/grpc_cluster_config_differential_test.rs) 对比 Rust 和既有 Go 二进制的12组结果（4组Put/Get、3组非法配置、1组request身份错误），并比较真实 follower 两种 RPC 的状态和文本。仅在断言真实集群 ID 后归零随机 ID，并将错误文本中的实际随机 cluster ID 替换为标识；副本数、错误类型及其余文本不归一化。

通过任务4只读 supervisor 运行。已有 `bin/pd-server` 的 build info 为 go1.26.8，没有 VCS revision，不能声称来自当前 HEAD。provenance、完整日志及只读报告随压缩证据保存。Go 与 Rust 都正常关闭，监督报告证明进程组和临时目录已清理。

初次聚焦测试编译可运行，真实 TCP 返回 Unimplemented，行为断言失败，退出101。首次实现后重启测试因使用新监听地址触发 IdentityConflict；修正测试复用身份地址后通过，未弱化生产身份校验。故障测试首次即通过。

## 验证命令

按1–10顺序原子 mkdir 动态领取槽位。使用槽位1与2；绝对 CARGO_TARGET_DIR 分别为 `/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`、`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。锁内记录计划、任务、会话、PID；子进程继承路径，无 target-dir 绕过。槽位1用于聚焦、包级及workspace测试；槽位2用于静态检查、构建及差分。阶段退出清理自有锁，保留缓存。

| 确切命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初次 `cargo test --locked -p astersql-pd-server --lib task_028_ -- --nocapture` | 101 | 1行为失败 |
| 最终同一聚焦命令 | 0 | 2通过，0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 134通过，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" bash scripts/run-pd-compat-readonly.sh --case cluster-config` | 0 | 1真实差分通过，0忽略 |

`cargo test --workspace --locked -- --test-threads=1`：退出0，1996通过，23忽略且不计通过；本任务真实差分已单独显式运行。

开始/结束Go保护集合1983文件（含ignored/untracked）路径、SHA-256及Git状态完全相同。监督报告 `compat-readonly-1793268411730705000.json` 的只读、输入未变、工作目录和进程组清理全部通过。plan.md 与HEAD逐字节相同；Python AST、证据链接、git diff --check通过。保留原有任务1–27删除，不提交无关变更。所有Cargo阶段结束释放自有锁并保留缓存。

[压缩证据](task-028-evidence.json.gz) 保存原始失败/通过/包级/workspace/差分日志、命令与槽位、源码哈希、完整Go快照及监督报告。SHA-256：`97d8e361b82fafa8e5a3469ab889e556190867852b84e68e77df2ca4c7c6af1a`。

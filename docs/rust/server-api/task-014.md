# 任务014：驱动 Store 配置同步 worker

状态：已完成（HTTP Store 配置同步、持久化发布及宿主生命周期边界）。

基准 HEAD：`7267ec6eb8f18bdd456e76851e651aeb068fd7fe`。依赖任务13的 worker/任期装配及真实只读差分证据已核查；任务1–3的 source/wire/surface 清单和任务4只读监督入口已读取。协议仍固定 kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与接线

- source-inventory.md 的任务14整行、`state_pipeline.rs`共同 owner 行、`cluster.go:489`启动行及 `runStoreConfigSync` 的14/95移交行：本次只覆盖配置同步，不扩张相邻 worker。共同来源的恢复/心跳/任期/统计分别沿用任务6–13证据；coordinator负责调度，不是该 job 的启动 owner。
- Go 只读映射：`server/cluster/cluster.go:719–870`，立即同步、每分钟重试、TiKV及Preparing/Serving筛选、失败候选回退、loopback状态地址解析、GET `/config`三秒超时、JSON解码和Adjust；`server/config/persist_options.go::SwitchRaftV2` 将 store-limit-version 切换至 v2。
- wire-inventory.md 没有该内部 worker 的独立 PD RPC/管理路由；本次通过真实 TiKV HTTP端点和共享MetaStore观察副作用，不宣称新增管理API已验收。
- surface-inventory.md 的 M-161 `pd_cluster_store_sync`及注册来源仍由15/86/101负责，本任务未增加指标，未把该指标计为已验收。TLS配置接线属于后续安全配置任务，本次验证 plain HTTP端点。

`server/cluster/store_config_worker.rs` 使用宿主executor、子取消令牌、立即首轮、60秒间隔和join；请求连接在同一future内驱动，不留下独立连接任务。超时/取消关闭socket。复用canonical StoreConfig解析和Adjust；Go即使HTTP状态非2xx仍解码body，本次也保留该行为。结构为数组、无效JSON、负unsigned字段均拒绝。连接主动关闭仍读取已缓冲完整响应。

`pkg/runtime/cluster_host.rs` 在每个leader任期接入worker，使用现有resources admission和canonical缓存。复用PersistedConfig和StorageEndpoint，以同一MetaStore保存全量动态配置；首次持久化使用已调整的默认配置，已有动态字段沿用恢复快照。raft-kv2切换同时保存v2 Store limit。HTTP/解码/存储失败不发布新配置，后续周期仍可重试。成功事务后才更新共享ProviderSnapshot；宿主shutdown取消并join，释放资源。

按任务要求，Rust在写入失败时保留旧活动配置，并将store和v2切换放在一个事务；Go源码先SetStoreConfig再分别Persist，不能把本次Rust存储失败测试宣称为Go同故障差分。Go/Rust运行差分覆盖实际HTTP成功、raft-v2持久化和HTTP错误保留旧配置。没有改写Go、协议模型或领域实现，没有建立第二个运行时/集群缓存。

## 行为证据

先建立可编译fetch入口和真实TCP `/config`测试；`cargo test --locked -p astersql-pd-cluster --lib task_014_ -- --nocapture`退出101，实际0通过/1失败/0忽略，断言“TiKV HTTP configuration must be fetched”，错误为尚无worker实现。未以编译错误作为失败证据。

最终cluster聚焦测试覆盖HTTP成功和派生大小256MiB、无效JSON及形状/类型、失败Store回退、Removed Store过滤、HTTP500合法body、受控时钟立即首轮/一分钟重试/stop后不再变更、请求中途取消后socket EOF、三秒超时后socket EOF、Connection: close完整响应。

runtime通过真实etcd和真实HTTP验证共享活动配置及`/pd/514/config`存储相同，raft-v2对应v2 limit。无效JSON保留旧配置；在真实配置Put事务边界注入Unavailable，活动值与原持久值保持256MiB；解除故障后更新为512MiB并持久化；预取消操作不再覆盖状态。shutdown后配置不可读、Registry为空、listener关闭、MetaStore引用回收。

只读dual监督器使用已有Go `bin/pd-server`，真实RPC Bootstrap携带TiKV状态地址，HTTP服务实际返回配置。Go `/pd/<cluster-id>/config`的store和store-limit-version与Rust场景相同；随后HTTP改为无效JSON，等待收到Go下一次真实GET的响应观察事件，再读取etcd证明旧配置保留；停止服务后listener关闭。未用mock调用次数代替网络和持久结果。只归一化集群ID、端口和时间原点；不声称外部etcd等同Go embedded-etcd拓扑。

Go二进制SHA-256：`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。既有二进制build info没有VCS revision，不声称来自当前HEAD；完整build info和输入哈希随只读报告保留。

## 验证记录

所有Cargo命令从仓库根、默认features运行。原子mkdir动态领取槽位1，记录任务/会话/PID；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。先运行`cargo fmt --all`；没有--target-dir覆盖。

| 确切命令 / 阶段 | 退出码 | 有效测试 |
| --- | --- | --- |
| 初始`cargo test --locked -p astersql-pd-cluster --lib task_014_ -- --nocapture` | 101 | 0通过/1行为失败/0忽略 |
| 最终相同聚焦命令 | 0 | 6通过/0忽略 |
| `cargo test --locked -p astersql-pd-cluster -- --nocapture` | 0 | 61通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_014_ -- --nocapture` | 0 | 1通过/0忽略 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case store-config` | 0 | 1真实双实现场景通过/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-cluster` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建，不计行为测试 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 1956通过（含2项doc-test）/9忽略；本任务差分已另行显式通过 |
| Python AST、`git diff --check` | 0 | 静态 |

完整原始日志、命令/退出码/摘要、最终源码哈希、Go文件快照及两次dual监督报告见[压缩证据](task-014-evidence.json.gz)，SHA-256 `c3f1fde2f24a9c7e700ac148d432543b54c5afb253564aee5c65f48194cc1173`。初始用offline命令刷新仅新增直接依赖边的Cargo.lock，随后用要求的--locked命令重新取得行为失败；没有新增或升级传递依赖。

回归时发现start_paused真实网络超时测试会在连接建立前自动推进虚拟时间：首次包级运行等待，主动SIGTERM后退出101；另一次旧测试版本workspace回归因提前EOF断言失败。最终改为真实连接收到请求后再暂停并推进三秒，聚焦、61项包级及完整workspace均通过。保留这些失败日志，不把中止计为通过。最终生产版本dual再次通过；其后只修改这一独立超时测试的时钟建立顺序，没有修改生产代码或差分场景。

Go只读守卫开始/结束均1983个文件（含ignored/untracked和槽位内既有Go文件），完整路径集合、每文件SHA-256及Git状态完全相同；排序快照摘要 `eafa2dc5f0470621e39729f6ab4572c6cc35be32ae5d02034510e0e746ba4cbb`。dual报告readonly_unchanged和inputs_unchanged为true，进程组及独占工作目录均清理。所有本任务Cargo、fixture、HTTP及Go子进程结束后释放自己的slot-1.lock；缓存保留。plan.md、来源史诗、Go及Go模块均无本任务差异，保留任务1–13的原有删除。

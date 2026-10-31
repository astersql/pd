# 任务095：关闭长流与后台 worker

基准 `6ea4dd40f`；依赖 [任务094](task-094.md) 已满足。Go 和模块文件、plan.md 只读。

当前生产装配已具备worker取消与join。新增组合验收后发现关闭竞态：成员监视先退出时，
node的serving分支仅等待角色清理而停止poll listener；阻塞lease revoke可让TCP继续准入。
本任务局部修复node.rs，在该分支同时join角色与listener，并显式记录两者完成状态，
避免后续重复poll已完成future。不创建重复shutdown管理器。
独立测试 [shutdown_workers_test.rs](../../../pkg/runtime/shutdown_workers_test.rs)
经 [lib.rs](../../../pkg/runtime/lib.rs) 显式注册。

## 来源与适用边界

[source-inventory.md](source-inventory.md) 的任务95行、心跳/同步适配准入行、
node.rs 导出/worker/持久化共享行及“Go集群worker责任核对”全部11行适用：

| Go worker/行为 | 当前 Rust 装配与关闭证据 |
| --- | --- |
| runServiceCheckJob | member_role_worker 的角色/成员监视与 node 的 serving scope 同时取消；独立MCS按任务078明确拒绝，未创建空worker |
| runMetricsCollectionJob | cluster_metrics_worker、ServiceMetrics worker 与 push worker 由节点取消并等待；Registry最终为空 |
| runNodeStateCheckJob | ClusterHost::shutdown 取消并等待 store_worker；保留 host 句柄但持久资源已释放 |
| syncRegions | RegionSyncer::stop 清空 downstream，响应流结束；follower同步由角色作用域取消 |
| runReplicationMode | DR领域仍按任务049/078移交；本任务不声称实现DR后台worker |
| runMinResolvedTSJob | resolved_worker.stop 在资源移除前等待 |
| runStoreConfigSync | config_worker.stop 在资源移除前等待 |
| runUpdateStoreStats | stats_worker.stop 在资源移除前等待 |
| startGCTuner | Go GC tuner 不适用于Rust；不安装Go runtime或全局调优器 |
| startProgressGC | progress_worker await，随后释放资源和指标 |
| runStorageSizeCollector | metering/keyspace 属任务078/102边界，未装配并明确拒绝；无伪造worker |

共享清单中的历史manifest/候选源码行不当作独立行为通过数。
[wire-inventory.md](wire-inventory.md) 中 classic Tso/RegionHeartbeat/SyncRegions/
WatchGlobalConfig 归各原owner；其共同“4/82/95”前置在本任务验收取消，
独立MCS/ResourceManager/Keyspace等仍遵循078领域边界。
[surface-inventory.md](surface-inventory.md) 中 listener/宿主Registry共享行适用，
不新增配置、指标或CLI。

Go只读参考：server/server.go::Close 先CAS停止准入，再stopServerLoop、关闭客户端/
hbStreams/storage；pkg/schedule/hbstream/heartbeat_streams.go::Close 取消并Wait；
pkg/syncer/server.go 的sender关闭路径等待发送组。
Rust复用 node.rs 的 child_token、PdNode::shutdown/wait、member_role_worker::retire、
ClusterHost::shutdown 和 pull-driven gRPC streams。worker与server全部退出后才释放作用域；
lease退出与Registry注销复用原有清理，不依赖客户端主动drop。
不宣称外部MetaStore等价于Go embedded-etcd拓扑。

## 新增行为证据

`task_095_behavior`：真实TCP/tonic/HTTP、真实etcd同一MetaStore，建立TSO响应、
空闲双向心跳、region同步及收到持久配置事件的watch。所有客户端及请求生产者
保持存活时，5秒内shutdown完成；再次shutdown安全；宿主token不取消。
检查真实HTTP health成功、watch持久值仍存在、leader key删除、downstream清空、
Registry为空、listener拒绝连接且原端口可重绑，保留旧host但snapshot拒绝，
释放stream后MetaStore强引用计数为1。

`task_095_host_cancel_and_interrupted_shutdown`：相同真实服务与长流，宿主取消后
poll并丢弃一次shutdown等待，随后再次shutdown仍能join并完成上述清理。

`task_095_stops_admission_before_lease_release`：仅在真实MetaStore revoke边界暂停，
确认实际leader key仍存在时TCP已停止准入，丢弃第一次关闭等待后放行真实revoke，
再次shutdown等待成功并确认leader key删除、Registry清空、宿主未取消。
该门控不合成返回值、不以调用次数验收。

首次有效基线组合场景1通过（退出0），但后补的lease门控在重复聚焦运行中捕获真实竞态：
2通过/1失败/1忽略，退出101，断言listener必须在后端回收完成前停止准入，实际超时。
该可编译、真实TCP/etcd失败是本任务生产修复的TDD证据；不是人为改坏基准。
此前测试草稿用了不允许的global-config路径，返回InvalidArgument，0通过/1失败、退出101；
这是测试输入错误，已改用真实允许路径，不作为生产缺陷/TDD失败证据。

专用 `task_095_readonly_shutdown_differential` 通过只读supervisor新场景
`--case shutdown-workers` 启动已有Go二进制，真实bootstrap后保留同样四条流，
记录其终止状态及关闭后TCP准入，再运行Rust组合场景。Go心跳先用可恢复的invalid-region响应
确认流已建立，再保持请求生产者存活；不把等待response headers误当作已建立的流。
Go二进制哈希/build info随只读报告保存，不声称它由当前HEAD构建。宿主Future/Registry是Rust
嵌入API，没有Go同构入口，不声称对Go实施这些检查。

## 验证记录

所有命令从仓库根执行，默认features，CARGO_BUILD_JOBS=4。早期与包级验证使用
RUST_TEST_THREADS=1；修复后的重复门控、workspace及最终四场景差分使用RUST_TEST_THREADS=4。
实际验证与差分动态领取slot-1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
两次独立格式化动态领取slot-2：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
各阶段均原子mkdir领取、记录任务/会话/PID、结束释放自己的锁，保留缓存。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_095_ -- --nocapture`（首次有效基线） | 0 | 1通过 |
| `cargo test --locked -p astersql-pd-runtime --lib task_095_ -- --nocapture`（最终） | 0 | 3通过；1默认忽略的差分另行执行 |
| `cargo test --locked -p astersql-pd-runtime --lib task_095_stops_admission_before_lease_release -- --nocapture`（修复后连续10次） | 0（每次） | 每次1通过、0忽略，共10次通过 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture`（最终包级） | 0 | 80通过、37默认忽略 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 包级检查 |
| `cargo check --workspace --locked` | 0 | workspace检查 |
| `cargo test --workspace --locked` | 0 | 2219通过、77默认忽略；包含修复后的生产逻辑和lease顺序测试 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 当前进程产物 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case sync-regions --case global-watch --case classic-tso` | 0 | 每场景1通过、0忽略，共3通过 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case shutdown-workers`（最终） | 0 | 1通过、0忽略，真实Go及Rust四流关闭 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case sync-regions --case global-watch --case classic-tso --case shutdown-workers`（最终当前实现） | 0 | 每场景1通过、0忽略，共4通过 |

关闭差分首次两轮分别在心跳/同步流建立时遇到BrokenPipe（各退出101、0通过/1失败），
按已有Go调用路径补齐非空region及member.client_urls后通过；两轮失败原始报告保留。
这是oracle场景建立阶段的输入修正，不作为关闭行为缺陷或TDD失败证据。
最终Go输入修正后发现并修复上述生产竞态；随后重新运行聚焦、包级、差分及workspace。
修复后全量回归的最后一个oracle场景还曾因关闭后读到已缓冲帧而失败（退出101、
该场景0通过/1失败；前三个场景均通过）。关闭不保证清空客户端已接受的传输缓冲，
故将oracle改为5秒总期限内持续读到终止，并逐流报告buffered帧数。
这是测试断言修正，没有改动生产代码；之后再次运行最终聚焦及全部四场景对照。

Go最终观测：TSO/心跳为`Unknown:h2 protocol error: error reading a body from connection`，
watch为EOF，sync为`Unavailable:region syncer stopped`。
Rust宿主关闭：TSO/心跳/watch为EOF，sync为Unavailable。
双方进程/宿主的关闭均终止所有四条活跃流并停止TCP准入；未把Go进程关闭的传输错误
与Rust宿主正常EOF宣称为逐字节/状态码等价。配置事件、同步数据、deadline和compaction
错误的具体协议兼容性另由本次重跑的三个既有差分覆盖。

原始日志、命令/退出码、有效数量、二进制/源码哈希、清单对应行、只读守卫及
supervisor进程组/工作目录清理报告见 [压缩证据](task-095-evidence.json.gz)。
最终Go/模块文件集合（含ignored/untracked）1101个路径及SHA-256不变，Go Git状态不变；
plan.md与基准逐字节相同。自有进程和锁均已清理，缓存保留；未提交其他任务或用户原有变更。

状态：已完成。

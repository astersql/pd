# 任务016：驱动 ResolvedTS 聚合 worker

状态：已完成。

基准 HEAD：`c12e1803e8f9b426b6ac0cd9d00a423f54a3db00`。依赖任务15的 [宿主与验证证据](task-015.md)、任务1–3清单及任务4只读入口已核查。保留任务1–15既有删除，未修改只读计划、Go或模块文件。

## 来源与实现

- [source-inventory.md](source-inventory.md) 的任务16整行、`cluster.go`/`state_pipeline.rs`共同owner行、488行启动点及 `runMinResolvedTSJob` 聚合/持久化/重启行由本任务覆盖。`pkg/schedule/coordinator.go`不拥有本job，调度采集不并入聚合。
- Go `server/cluster/cluster.go:311–318,2643–2705`：只有初始化集群可聚合（超过一个region，或起始空key的region满足配置voter数且没有pending peer）。仅纳入`IsAvailableForMinResolvedTS`：未移除、有leader的TiKV；Removing仍纳入，离线但仍有leader的store不能因不可达而丢弃。全空、MAX和不前进值不写。
- 复用 [store_health.rs](../../../pkg/core/store_health.rs) 的资格谓词及 [timestamp_values.rs](../../../pkg/storage/endpoint/timestamp_values.rs) 的十六进制持久化，未复制领域/编码实现。当前值在写前更新；写失败不回退，同值不重试，下次更大值才写，保持Go行为。
- [resolved_ts_worker.rs](../../../server/cluster/resolved_ts_worker.rs) 使用宿主executor、取消令牌和可控时钟；首轮等待配置间隔，零值时每秒重读配置且不聚合、不写。每轮完成后重置等待。单worker串行更新；取消停止新轮并join已接纳事务。库不创建runtime或全局状态。
- [cluster_host.rs](../../../pkg/runtime/cluster_host.rs) 从同一MetaStore恢复值、使用同一canonical state和恢复的PDServerConfig周期；在任期内验证leadership后聚合，shutdown先join再释放资源。恢复/验证/持久化错误可通过`resolved_ts_error`观察。
- [wire-inventory.md](wire-inventory.md) 的ReportMinResolvedTS归任务34，min-resolved-ts HTTP归77；本任务没有提前实现这些Rust协议入口。差分用Go真实RPC/HTTP驱动，Rust通过宿主公共入口和共享MetaStore观察同一行为。固定kvproto commit仍为`f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。
- [surface-inventory.md](surface-inventory.md) 的`pd-server.min-resolved-ts-persistence-interval`默认1秒由既有配置库提供，通用动态配置API归47/90；worker本身每tick读取提供方，已有宿主从恢复配置接线。没有新增独立配置缓存或协议类型进入领域层。

## 行为与差分证据

初始用可编译空刷新入口运行聚焦命令，退出101：2项实际行为测试失败、0通过/0忽略；`eligible stores must advance cluster minimum`实际0、期望42。没有把编译错误或零测试算作失败证据。

最终 [独立测试](../../../server/cluster/resolved_ts_worker_test.rs) 4项验证真实store集合、初始化门槛、Removing/Removed/无leader、单调前进、MAX/零/空候选、可控时钟零间隔与启用、取消释放引用以及等待已接纳持久化。真实etcd停止使写60失败，内存保持60；重启后同值不重试，持久值仍42，61才成功写入。存储故障在实际事务边界发生，没有以mock调用数证明结果。

[宿主测试与差分](../../../pkg/runtime/resolved_ts_worker_test.rs) 验证真实etcd恢复20、周期聚合并持久化42、停止后持久值保留。单成员差分通过任务4监督入口启动已有Go PD：真实Bootstrap、配置副本数为1、RegionHeartbeat提供leader、ReportMinResolvedTS报告42、HTTP读取42，重启同一数据目录后仍42。Rust同样使用副本数1、一个region和store，通过公共宿主读取及同MetaStore持久值观察42；不宣称Rust的34/77传输接口已完成，或外部etcd与Go嵌入拓扑等价。

差分首轮发现Go未满足初始化门槛，HTTP返回MAX；据此补齐门槛并使两端使用相同副本数。随后一次配置请求缺少`replication.`前缀返回400，一次测试重复stop导致restart helper报SIGTERM失败；修正测试驱动后最终通过。以上失败未作为成功证据。故障和取消来自Rust真实测试，没有声称运行了Go同故障差分。

## 验证记录

实际动态原子mkdir领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。所有Cargo及fixture/差分子进程继承该绝对路径，不覆盖target-dir。结束清理自己的占用记录和锁，保留缓存。

| 确切命令 | 退出码 | 实际测试数 |
| --- | --- | --- |
| 初始`cargo test --locked -p astersql-pd-cluster --lib task_016_ -- --nocapture` | 101 | 2失败/0通过/0忽略 |
| 最终相同聚焦命令 | 0 | 4通过/0忽略 |
| `cargo test --locked -p astersql-pd-cluster -- --nocapture` | 0 | 69通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_016_ -- --nocapture` | 0 | 1通过/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-cluster` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case resolved-ts` | 0 | 1真实双实现场景通过/0忽略 |
| 首次`cargo test --workspace --locked` | 101 | 既有task006端口重绑`AddrInUse`；本任务测试通过，未以部分通过算全量成功 |
| 重跑`cargo test --workspace --locked` | 0 | 1966通过（含2项doc-test）/11忽略 |
| 补强既有task_016_behavior的初始化门槛断言后，重跑上述聚焦及包级命令 | 0 | 分别4及69通过/0忽略 |

workspace成功后仅补强已有测试的断言（已有合格store但副本不足时必须返回空候选），生产代码未变；再次执行格式检查、聚焦、包级及只读dual差分。没有把忽略的兼容测试当作自动通过；其中ResolvedTS差分由监督入口以`--ignored`明确执行且恰好1通过。

原始命令日志、首次失败及重跑结果、监督报告与源码哈希、Go完整守卫快照见[压缩证据](task-016-evidence.json.gz)。最终dual的`readonly_unchanged`及`inputs_unchanged`均为true，进程组和工作目录清理成功。Go既有二进制的构建信息没有VCS revision，不声称来自当前HEAD。

开始/结束全仓Go/模块文件1983个（含target、ignored、untracked），路径与每文件原字节SHA-256完全一致，排序(path,hash) JSON摘要`1460a6ab17b4c2b8b0643fefda210c69bcacb4b5a1a6dbd3c74400060d39d6c0`。开始及结束Go/模块Git状态没有差异。Python AST、文档局部链接、`git diff --check`通过；未改plan.md、Cargo.lock或模块文件。仅提交本任务实现、测试、入口扩展和证据，不提交编号任务文件或其他任务的既有删除。

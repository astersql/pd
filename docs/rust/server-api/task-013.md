# 任务013：驱动Store统计更新worker

状态：已完成（周期Store统计刷新与leader任期装配边界）。

基准HEAD：`c0d339f17`。依赖任务12已有真实worker、leader任期、共享缓存与只读Go差分证据；任务1–3的source/wire/surface清单和任务4只读入口已核查。协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与实现

- source-inventory.md任务13整行、`state_pipeline.rs`共同owner行、cluster.go:490启动worker行及`runUpdateStoreStats`的13/95移交行。
- Go只读映射：`server/cluster/cluster.go::runUpdateStoreStats`每9ms调用`BasicCluster.UpdateAllStoreStatus`；`pkg/core/basic_cluster.go`从区域索引刷新七项计数，跳过Removed Store。复用Rust canonical `BasicCluster::update_all_store_status`，不重放Store heartbeat或rolling样本。
- wire-inventory.md无独立worker协议，沿用真实RegionHeartbeat；Go通过现有HTTP Store状态观察计数，Rust通过同一ClusterHost缓存观察。HTTP Store适配完整实现仍由其owner负责。
- surface-inventory.md的`M-158 update_stores_stats_time`构造/注册归任务15/86/101；本任务没有新增配置或指标，也不宣称这些指标已经验收。coordinator不是本job的启动owner，不复制调度领域实现。

`server/cluster/store_stats_worker.rs`复用既有周期任务的宿主executor、首轮等待、Skip missed ticks、取消和join；9ms周期与Go一致。`refresh_store_stats`仅调用已有领域API。`pkg/runtime/cluster_host.rs`在每个leader任期启动独立统计worker，使用相同resources admission锁、共享state以及leadership校验。shutdown取消并等待两个worker，随后清理syncer、runners和HotStat；worker join错误也不会跳过资源清理。没有新runtime、proto领域依赖或第二份缓存。

该job本身没有持久化事务：计数是区域索引派生值，持久元数据由已有heartbeat effect负责。错误场景在真实RPC边界发送旧epoch报告，断言错误及既有统计不被99MiB旧值覆盖；不为无I/O的刷新函数伪造存储失败。

## 失败与通过行为

聚焦测试先打通可编译接口，刷新尚为空实现时实际运行1项，tick后断言region_count应为1但实际为0：退出101、0通过/1失败/0忽略。实现调用canonical领域API后相同测试通过；未以编译失败代替行为失败。

最终cluster聚焦2项：受控Tokio时钟验证8ms不刷新、10ms后刷新、重复tick不累加、stop/join后推进1秒不再更新且Arc释放；空Serving Store清零、Removed Store保持旧计数、旧Arc快照保持不变。

runtime真实etcd+TCP/tonic场景发送正常5MiB心跳、重复心跳以及99MiB旧epoch心跳，旧epoch返回Unknown/stale；共享缓存计数为`[leader_count=1, region_count=1, leader_size=5, region_size=5]`，区域元数据真实持久化。更多tick后值保持一致。shutdown后手动设置缓存region_count=77，等待50ms后仍为77；listener关闭、Registry清空、MetaStore引用回收。

只读dual入口复用已有Go `bin/pd-server`，真实Bootstrap后执行同一个正常/重复/过期heartbeat函数。Go HTTP `/pd/api/v1/store/1`观察到的四个计数与Rust共享缓存完全相同，旧epoch错误类别及stale语义也相同。没有归一化这些计数。Go二进制build info和SHA-256随监督器证据保存；不声称无VCS revision的既有二进制来自当前HEAD，也不声称外部etcd拓扑等同Go embedded-etcd。

首次dual测试先等待stream response再发送请求，Go成功心跳没有即时响应，导致测试驱动等待；主动SIGTERM监督器，退出130并清理进程组/目录。修正为先发送请求和旧epoch请求，并给RPC与读取设置5秒超时，最终dual退出0、1项通过/0忽略。中止运行不计为差分通过。

## 验证与只读守卫

所有Cargo从仓库根、默认features执行，Rust修改后先`cargo fmt --all`。原子mkdir动态领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。owner记录包含任务、会话及持有shell PID，缓存保留。

最终命令、退出码及有效测试数见下方最终核验记录；完整原始输出与Go二进制/输入哈希在压缩证据中。

开始对target外1101个Go/模块文件记录存在性、SHA-256及Git状态；补充全仓库含ignored/target共1983项后，与任务12结束的完整守卫逐项相同。最终再比较该完整集合与Git状态。初始target外清单不冒充含target的全量快照；只读监督器另对完整1983项保存开始/结束守卫。未运行Go写入入口、make或failpoint命令；已有Go fixture构建遵循任务4只读调用链。

未修改只读plan.md、来源史诗、任何Go文件或Go模块；保留任务1–12既有删除，不纳入本任务提交。

## 最终核验记录

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-cluster --lib task_013_ -- --nocapture`（空刷新接口） | 101 | 1失败/0忽略 |
| 同一聚焦命令（实现后） | 0 | 2通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_013_ -- --nocapture` | 0 | 1通过/0忽略；最终旧epoch增强另由包级覆盖 |
| `cargo fmt --all -- --check` | 0 | 格式 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-cluster` | 0 | 静态 |
| `cargo test --locked -p astersql-pd-cluster -- --nocapture` | 0 | 55通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 18通过/7需oracle忽略 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case store-stats` | 0 | 1通过/0忽略 |

| `cargo test --workspace --locked` | 0 | 1949通过/8需oracle忽略 |

最终完整Go守卫1983项：存在性、每项SHA-256和Git状态全部一致；target外初始1101项也逐项匹配。监督器`inputs_unchanged`、`readonly_unchanged`、`work_directory_cleaned`均true。所有Cargo、fixture、Go/Rust服务及worker进程已结束，自己持有的slot-1 owner记录和锁目录已删除，槽位缓存保留。`git diff --check`通过。

完整失败/通过日志、确切argv、退出码与测试摘要、源码哈希、Go守卫及监督器报告见[压缩证据](task-013-evidence.json.gz)。SHA-256：`0f5dfa6581860c14ec731298bbeb85c8c06e544c6bcbcc21d9cb3165f74b030a`。

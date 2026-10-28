# 任务012：驱动Store状态检查worker

状态：已完成（Store生命周期检查与leader任期装配边界）。

基准HEAD：`7f90a12dbdd9f95bae09fd23b1bda89db8c4d535`。任务1–3清单和批次11证据已核验；主题6准入沿用task-001。协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与实现

- source-inventory.md任务12整行、state_pipeline.rs共同owner行、cluster.go:485的worker行、runNodeStateCheckJob的12/95移交行。
- Go只读映射：cluster.go::runNodeStateCheckJob、checkStores、checkStore、getThreshold、calculateRange、ReadyToServeLocked、BuryStoreLocked、setStore、deleteStore；pkg/schedule/coordinator.go::ShouldRun/GetPrepareChecker与既有prepare gate。
- wire-inventory.md没有独立worker协议；复用既有GetStore观察同一host缓存。surface-inventory.md的配置/指标共享归属不因本任务完成而整体验收；没有新增配置键或指标。进度展示、完整指标、Store管理API及调度配置持久化仍按各自owner验收。

新增server/cluster/store_state_worker.rs，独立测试在lib.rs显式注册。复用canonical BasicCluster、StoreInfo状态映射、Store时间API、拓扑权重、RuleManager与StorageEndpoint。Preparing使用heartbeat减start的uptime、初始集群小于100区域的快捷条件，或准备完成后的实际placement/topology阈值；物理销毁的Store不晋升。非空Removing保持原状态，空Removing持久化为Removed；Removed仅在down time严格超过30天后删除。没有根据失联时间自动下线Serving Store；Go的这个job也不做这种转换。

先成功持久化，再发布缓存状态；事务失败保留旧缓存并继续处理其他Store，下次tick可重试。状态发布合并进当前Store快照，保留心跳统计；墓碑化/删除清理placement缓存。runtime清理Removed Store的rolling统计，并提供最近一轮错误供宿主读取。

ClusterHost每个leader任期在宿主executor上启动10秒周期worker，复用资源admission锁与带任期校验的TermStore。首轮检查等待完整周期；取消后join当前工作，再关闭syncer、其他workers和统计资源。即使worker join失败，资源清理仍执行。没有私有runtime、全局信号或第二份领域状态。PrepareChecker复用已有实现和同一BasicCluster，不新增区域收集器。

必要局部扩展包括runtime的cluster_host.rs、node.rs、cluster_restore.rs及独立测试。placement rules在创建后台资源前通过现有API初始化，初始化期间续租；初始化失败不会遗留已启动的HotStat。未持久化配置也调用已有schedule/replication调整API，修复准备时限0及副本数0，得到Go的48小时/3副本默认值；已有无placement默认和持久配置覆盖逻辑保留。runtime仅增加对既有canonical schedule crate的直接依赖，Cargo.lock只追加该依赖边，没有新版本或新crate。

## 失败与行为证据

初始task_012_behavior先编译并运行1项，真实tick后断言Preparing应为Serving失败，摘要0通过/1失败。最初包装命令以tail结束，包装shell退出0，未单独捕获Cargo退出码；该0不作为通过证据，原始断言日志完整保留。

新增task_012_default_preparing_config另取到确切失败码101：1项运行，实际准备时限0而期望172800000000000纳秒。修复后同一命令退出0、1项通过。受控时间测试最初在Tokio毫秒计时边界只yield一次，尚未执行tick；改为跨过1毫秒并调度就绪工作，仍分别断言9秒不运行、10秒后的tick转换、取消join后推进100秒不写入。

最终cluster聚焦4项实际行为：

1. 真实etcd上的周期tick晋升Preparing并墓碑化空Removing，持久元数据与缓存一致。
2. 受控Tokio时钟验证首次间隔、真实状态转换与取消join后的不可再写入。
3. 停止真实etcd，在保存/删除事务边界取得3个错误，旧缓存不变；重启并重连后重试成功。严格30天边界、非空Removing及物理销毁Preparing均有实际断言。
4. 100个真实区域、非零区域大小、准备gate、已初始化placement rules和无placement拓扑路径，验证不足数据保持Preparing、足够数据晋升；uptime边界取heartbeat而不是墙钟。

runtime新增2项：默认配置验证；真实TCP/tonic GetStore观察Serving/Removed及etcd持久状态，随后向同一缓存放入待转换状态并shutdown。再等待11秒，缓存及持久元数据均未被worker写入；listener关闭、Registry清空、MetaStore强引用恢复为1。现有角色切换、心跳、同步等回归也全部通过。

只读监督器增加store-state场景。复用已有Go bin/pd-server，Bootstrap后注册额外Store，通过正常HTTP DELETE将空Store下线，再由真实10秒job墓碑化；同一observe函数对Go/Rust核对GetStore状态和etcd持久元数据，Go另检查HTTP health。首次Go场景只有3个在线Store，正常下线因副本数保护返回400；增加第4个在线Store后重跑成功，没有用force绕过保护。早期场景运行期间输入还发生变化，因此只采用最终固定输入下的证据。

最终dual场景1项通过/0忽略，两边状态均为`[Serving, Removed]`，元数据副作用一致。真实错误、GC时间边界与阈值由上述Rust/etcd行为测试及只读Go源码映射覆盖；本次Go运行差分针对晋升和空Store墓碑化，不把源码推导写成其他分支的运行差分。Go二进制build info没有VCS revision，不声称它来自当前HEAD；二进制哈希和完整build info随证据保存。Rust使用外部etcd不代表与Go embedded-etcd拓扑等价。

## 最终验证

全部Cargo从仓库根、默认features执行，修改后先执行`cargo fmt --all`。依次原子mkdir动态领取槽位1，记录本会话及持有进程PID；实际`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。新增直接依赖时以`cargo check --offline -p astersql-pd-runtime`更新既有lock条目；最终均使用下列locked命令。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-runtime --lib task_012_default_preparing_config -- --nocapture`（默认值修复前） | 101 | 1失败/0忽略 |
| 同一默认配置命令（修复后） | 0 | 1通过/0忽略 |
| `cargo fmt --all -- --check` | 0 | 格式 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo test --locked -p astersql-pd-cluster --lib task_012_ -- --nocapture` | 0 | 4通过/0忽略 |
| `cargo test --locked -p astersql-pd-cluster -- --nocapture` | 0 | 53通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 17通过/6需oracle忽略 |
| `cargo check --locked -p astersql-pd-cluster` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 1946通过/7需oracle忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 二进制构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case store-state` | 0 | 1通过/0忽略 |

Go只读守卫开始/结束均1983项，覆盖ignored/untracked与所有Go模块文件：存在性、每项SHA-256和Git状态完全相同。最终监督器readonly_unchanged、inputs_unchanged及work_directory_cleaned均true。无Go/模块文件、只读plan.md或来源史诗改动；保留原有任务1–11删除并排除提交。

所有Cargo、服务和槽位产物进程已结束，自己持有的slot-1 owner记录和锁目录已删除，缓存保留。按要求标记任务12完成并删除编号文件，任务实现和证据独立提交。

原始失败/通过输出、确切命令与退出码、源码哈希、Go守卫、监督器报告及清理记录见[压缩证据](task-012-evidence.json.gz)。

压缩证据SHA-256：`ccb363bb3e987ea695f21227243f0364eb8795dbd95ba569116cce7d58447bc0`。

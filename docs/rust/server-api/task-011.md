# 任务011：装配Region同步服务

状态：已完成（SyncRegions真实runtime接线边界）。

基准HEAD：`b73dfc331`。批次10及此前证据存在，主题6准入沿用task-001核验。协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与装配

- source-inventory.md任务11整行、共同心跳/同步准入行，以及syncRegions的owner=11/95移交行；本任务处理服务端装配，主动跟随leader与join留给95。
- wire-inventory.md的RPC-SyncRegions行：真实TCP/tonic双向流上的快照、增量、续传、EOF、错误与取消。复用canonical RegionSyncer、Downstreams与StatePipeline，不复制同步算法。
- surface-inventory.md中sync_transport、syncer测试与源码的安全/限流/配置/指标移交仍属于15/86/90/91/92/95/101；本次没有新增配置或指标，不将这些共享文件的所有来源行计作完成。
- Go只读oracle：server/grpc_service.go::SyncRegions；pkg/syncer/server.go::Sync、syncHistoryRegionLocked、syncFullRegionsLocked、bindStreamForSync、runDownstreamSender；pkg/errs/errno.go::ErrMismatchClusterID。相邻Rust证据由pkg/server/sync_transport_test.rs和pkg/syncer独立测试覆盖。

ClusterHost为每个leader任期创建一个RegionSyncer，绑定已有HeartbeatEffectsState；心跳pipeline使用该syncer的Downstreams，保持同一缓存、历史和history cursor。history cursor是区域变更序号，不是MetaStore revision；全量snapshot offset也独立于它。沿用canonical admission gate和历史发布worker的锁顺序，没有引入proto到领域层。Application和RoleServices提供拥有所有权的任期句柄，兼容已有静态借用接口；并发角色切换不会返回悬空引用。retire/shutdown取消并join同步流，再回收原有workers和存储。ClusterHost只持Weak句柄，读者不会在shutdown后隐式保留同步服务。

现有文件足够完成局部接线，因此未创建冗余wire_sync_regions.rs。实际生产变更在cluster_host.rs、node.rs、member_role_worker.rs、server/lib.rs和sync_transport.rs；没有新增依赖或修改Cargo.lock。

## 失败与行为证据

新增wire_sync_regions_test.rs，在lib.rs显式注册。首次聚焦命令编译成功，运行1测试，因runtime返回Unimplemented（region sync service is not configured）而失败，退出101。接线后真实测试通过。

补充半关闭场景后再次运行1测试，响应流错误关闭为Unavailable而失败，退出101。Go只读取首帧建立订阅；修正Rust持有请求流但不因请求半关闭取消响应流。收到首帧后的后续请求也不创建另一订阅。响应丢弃、host取消或任期结束仍触发关闭和join。

第一次真实双实现比较发现Go错误cluster返回FailedPrecondition而原适配返回Unknown，差分退出101；依据真实运行及errno.go修正Rust状态码。早期一次oracle运行还暴露Go RegionHeartbeat直到首个响应才发送headers；测试改为并发驱动心跳与读取持久状态。该早期运行被监督器取消，输入在运行期间发生变化，因此其inputs_unchanged=false，不能算通过证据；最终验证在固定输入下重新执行。

最终task_011_behavior在真实etcd MetaStore、宿主Tokio和同一TCP listener上：

1. RegionHeartbeat版本2实际持久化后，全量同步返回同一region/epoch/peer/leader及flow stats，并以独立history cursor完成。
2. 在线版本3增量后保存exclusive cursor；断线期间提交版本4，使用保存cursor重连，实际收到版本4重放；请求半关闭后仍收到版本5在线增量。增量起始cursor连续，防止把snapshot offset或etcd revision当作恢复位置。
3. 越界cursor触发版本5全量快照与正确completion；错误cluster为FailedPrecondition；首帧前请求EOF正常结束；过期版本1产生心跳错误且不增长同步history。
4. 持久region元数据和host snapshot一致；取消host后流返回Unavailable，downstream为空，宿主join完成，listener关闭，Registry为空；释放测试显式持有的最后syncer后，MetaStore引用恢复为1。

独立task_011_role_retirement在真实etcd撤销leader lease；旧同步流返回Unavailable、旧host拒绝读、旧downstream清空；新任期成功建立新流，shutdown后再次清理Registry和存储引用。没有私有runtime、全局subscriber或测试mock计数。

只读监督器新增sync-regions场景。复用既有bin/pd-server启动真实Go PD，bootstrap同样的store/region；同一exercise函数对Rust和Go执行上述快照/增量/重放/半关闭/越界/错误/EOF/过期报告。每个frame断言region元数据、leader、flow stats、header和cursor；双方通过真实etcd读取相同版本持久化副作用，Go另检查HTTP health与region epoch。双方活动同步流在服务停止后返回Unavailable。只归一化随机cluster ID、listener及初始history基点，比较cursor连续性，不忽略状态码差异。

Go配置use-region-storage=false以便检查同一etcd持久元数据；这不代表外部etcd和Go embedded-etcd拓扑等价。已有Go二进制build info无VCS revision，不声称它来自当前HEAD。主动follower同步、认证/限流/审计和完整指标兼容仍按来源清单移交。

## 验证记录

所有Cargo命令从仓库根、默认features执行。原子mkdir依次领取共享槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。命令、退出码、测试数、Go守卫及监督器完整报告随压缩证据保存。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-runtime --lib task_011_ -- --nocapture`（初始未接线） | 101 | 1行为失败/0忽略 |
| 同一聚焦命令（补充请求半关闭） | 101 | 1行为失败/0忽略 |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_011_ -- --nocapture`（最终） | 0 | 2通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 15通过/5需oracle忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo fmt --all -- --check` | 0 | 格式 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked` | 0 | 1940通过/6需oracle忽略 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 真实二进制构建 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case sync-regions` | 0 | 1通过/0忽略 |
| `git diff --check`及Python AST解析 | 0 | 静态 |

开始/结束完整1983项Go/模块文件清单包含ignored/untracked与target缓存，存在性、SHA-256和Git状态完全相同。最终监督器readonly_unchanged、inputs_unchanged、work_directory_cleaned均true，进程组已清理。没有改动Go/模块文件、plan.md或来源史诗；原有任务1–10删除保留且不纳入提交。

全部验证及槽位产物进程已结束，自己持有的slot-1记录与锁目录已释放，缓存保留。按要求删除任务11文件。

原始命令、失败/通过输出、Go守卫和监督器报告见[压缩证据](task-011-evidence.json.gz)，SHA-256：`76065721d0756cfd41ad6bc8b171c220db7b4911e20614515a4c1ca445cd38ca`。

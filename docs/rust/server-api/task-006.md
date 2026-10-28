# 任务006：恢复持久化集群视图

状态：已完成（本任务持久化集群恢复边界）。

基准HEAD：`c6ecd3f63be9ed870ec539a16b7d6eca72789e3b`。依赖任务1–5证据和source/wire/surface三份清单已核对；任务5提供的canonical共享集群、真实etcd fixture和任务4只读入口可复用。本任务保留任务1–5的既有删除，不修改plan.md、来源史诗、Go源码或Go模块。

## 实现和来源映射

- [cluster_restore.rs](../../../pkg/runtime/cluster_restore.rs)复用StorageEndpoint的load_stores（生命周期兼容映射、权重）、load_regions（分页、epoch准入及overlap删除）和server-config的PersistedConfig::reload（默认值、旧字段迁移、typed JSON校验）。恢复的schedule/replication/store/label-property/key-type/flow配置注入唯一HeartbeatEffectsState，完整动态配置保留给宿主读取；不复制领域算法或协议codec。
- [node.rs](../../../pkg/runtime/node.rs)在请求collector、HotStat/runner和listener准入前完成恢复。恢复期间在同一future中续租，完成后验证leadership；取消、坏数据、读失败或lease丢失均返回启动错误，取消子scope并resign，监听器随失败返回释放。没有新增后台恢复worker或私有runtime。
- [cluster_host.rs](../../../pkg/runtime/cluster_host.rs)的只读服务读取与心跳共用的canonical cluster；server边界增加GetStore/GetRegionByID基础读取并由SyncGrpc委托，转换复用既有core适配器。不存在store返回Go形状错误header，不存在region为空；bucket查询明确Unimplemented。完整查询、bootstrap gating、follower/forwarding/认证等仍归任务22/23等owner，本任务不解除其验收。
- 独立[cluster_restore_test.rs](../../../pkg/runtime/cluster_restore_test.rs)在runtime/lib.rs显式注册。已有node_test的“尚未实现RPC”反例改用仍未实现的GetAllStores，因为GetStore已用于本任务恢复验收。
- source-inventory的“6 恢复持久化集群视图”整行、最小运行时owner6准入和LoadClusterInfo/端点来源对应本证据；wire-inventory各RPC共同“5–7集群角色”仅解除恢复前置部分，GetStore/GetRegionByID仅覆盖恢复后的基础读取；surface-inventory的配置reload共同前置、listener与宿主Registry仅覆盖本任务恢复/错误/取消边界，不宣称配置管理API或全量指标完成。

Go只读参考：server/cluster/cluster.go::LoadClusterInfo在LoadMeta后依次ResetStores/LoadStores/TryLoadRegionsOnce，失败阻止创建RaftCluster；server/grpc_service.go::GetStore/GetRegionByID提供网络观察；pkg/storage/endpoint/meta.go、config.go及server/config/persist_options.go固定存储和配置reload契约。Rust复用现有对应端点和配置crate。固定kvproto commit仍为`f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## TDD、故障与差分

初始测试成功编译并运行1项，在ready后断言`ready must include durable stores`失败：实际0、预期1，退出101；已在断言前关闭节点。初次配置fixture误用JSON boolean，按Go的`,string`持久化字段改为字符串；同身份重启需要保留成员client URL，测试据此复用端口。这些测试装配错误不计产品行为失败证据。

聚焦测试覆盖：真实etcd预存store/权重、region/epoch和配置；两次全新节点恢复；ready后的宿主快照、真实TCP GetStore/GetRegionByID与存储相符；region leader为易失状态不恢复；未知store错误header和缺失region；真实HTTP健康；配置max-replicas=5/key-type=raw；关闭后读拒绝、端口可释放、Registry清空、MetaStore强引用归还及父取消令牌隔离。坏store/region protobuf、坏JSON及非法key-type均阻止start；删除坏键后相同身份/端口/Registry立即重启。RecoveryGate只控制实际恢复读边界，其余事务和lease均委托同一真实etcd；覆盖挂起读时父取消、Unavailable读和真实revoke导致恢复期间续租失败，不用mock调用次数断言。

只读supervisor增加`--case cluster-restore`（必须dual）。已有Go二进制先bootstrap，再经同一StorageEndpoint模式写入元数据及动态配置；停止进程、用原数据目录/网络身份重启。Rust两次全新PdNode使用同一真实MetaStore。两侧真实RPC与配置/健康观察归一化后相同：

```json
{"address":"127.0.0.1:29999","healthy":true,"key_type":"raw","leader_absent":true,"region":7,"replicas":5,"store":1,"version":3}
```

实际Go结果还固定了错误边界：损坏持久化region后重启，日志报告ErrProtoUnmarshal/failed to load cluster info，GetStore拒绝服务；Rust同类数据使start失败并释放资源。Go进程保留embedded-etcd并重试选主，因此不能要求其通用health/member探针达到ready，也不能把Rust返回启动错误说成相同进程退出策略。初版对此错误使用成功ready探针导致预期超时，原日志保留；最终按实际cluster拒绝与错误日志验收。

Go显式持久化`pd-server.use-region-storage="false"`，因为reload会覆盖TOML选择。外部etcd是本任务已有过渡边界，不宣称Go默认LevelDB、embedded-etcd拓扑或主题10等价。正常恢复差分与损坏数据后的拒绝均为真实运行，非源码推导；Go二进制build info没有VCS revision，不能声称来自当前HEAD。

## 最终验证

所有命令在仓库根、默认features执行；实际原子领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁内记录计划/任务/会话及宿主/验证进程PID。Cargo、supervisor、Go/Rust服务及fixture全部结束后，核对无存活槽位二进制，删除自己的owner并rmdir释放锁；编译缓存保留。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| 初始`cargo test --locked -p astersql-pd-runtime --lib task_006_ -- --nocapture` | 101 | 0通过/1行为断言失败 |
| 最终同一聚焦命令 | 0 | 3通过/0忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 8通过/2需oracle忽略；已在最后一行显式补跑 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all`、`cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 1933通过/3忽略；2个runtime oracle和TSO均已显式补跑 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 最终独立二进制 |
| `PD_COMPAT_MODE=dual PD_COMPAT_GO_BIN=$PWD/bin/pd-server PD_COMPAT_RUST_BIN=$CARGO_TARGET_DIR/debug/pd-server bash scripts/run-pd-compat-readonly.sh --case cluster-restore --case cluster-host --case health-member --case tso` | 0 | 四场景各1通过/0忽略 |
| `bash -n scripts/run-pd-compat-readonly.sh`、Python AST、`git diff --check` | 0 | 静态检查 |

新增runtime依赖现有server-config及core-constant后，执行`cargo check --offline -p astersql-pd-runtime`（0）仅更新Cargo.lock的两个直接依赖条目；最终全部命令均使用规定的locked入口。没有下载/生成Go源码或改动模块。

中间错误均保留在证据：配置fixture编码/成员URL修正；续租future的Rust借用作用域编译错误修正；旧node_test对GetStore的Unimplemented断言更新；一次包级回绑AddrInUse后，最终默认并发包级及全量命令均通过；首次Go缺region因持久化region存储选项覆盖TOML修正；错误ready探针超时修正。一次中间差分期间修改续租源码，入口正确标记inputs_unchanged=false，该轮不作通过证据；最终四场景执行期间输入完全稳定。全量总数含既有Rust单元/反例等，不把它们全部当作Go差分。

原始日志、命令/退出码/测试摘要、源码与二进制输入哈希、前后完整Go清单和只读supervisor报告见[压缩证据](task-006-evidence.json.gz)，SHA-256：`1b2eba553d6004fad32ff97e26a585f7e4f38e66d151f1b83de64f26b9284cac`。

开始/结束Go与模块集合均为1983文件（包含ignored/untracked），路径、存在性和每文件SHA-256全部相同；排序(path,hash)紧凑JSON摘要均为`bff6436c8bac2e3448952c006db577aea404aa43af6e5fd862310c6df86c3ac0`，Go/模块Git状态均无差异。plan.md与HEAD字节一致。最终supervisor的readonly_unchanged、inputs_unchanged、四个进程组清理、独占工作目录清理全部为true。

真实Go二进制SHA-256：`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；最终Rust独立二进制SHA-256：`010cca198e566b7342a614682b3a504ec9ef76b7cce7bbfe557ad7d091e87b3b`。恢复差分直接装配当前runtime库；health/member及TSO另外覆盖最终独立二进制。

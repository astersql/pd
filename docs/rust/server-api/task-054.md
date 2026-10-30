# 任务054：Store限额与进度HTTP

基准 `728fff4a9`；依赖[任务053](task-053.md)已核验。Go、Go模块文件和 `plan.md` 只读。

## 来源与实现

- [source-inventory](source-inventory.md)：`server/api/store.go` 的 SetStoreLimit、SetAllStoresLimit、GetAllStoresLimit、Progress、GetStoresProgress；`server/handler.go` 与 `server/cluster/cluster.go` 的限额/进度调用链；owner 54 的 `pkg/progress/` 全部15项基准条目。
- [wire-inventory](wire-inventory.md)：HTTP-068、071、072、073。
- [surface-inventory](surface-inventory.md)：router.go:231、236–238。审计仍由任务81负责，本任务不宣称审计等价。
- [HTTP适配](../../../pkg/server/http_store_limit.rs)、[独立网络测试](../../../pkg/server/http_store_limit_test.rs)、[宿主写入](../../../pkg/runtime/store_limits.rs)。复用共享 listener、leader 转发、请求取消、Coordinator 资源锁及 leader-fenced MetaStore。

限额复用现有配置事务与限流器，不在 GET 中制造默认数据。保留 Go 的校验顺序、ttlSecond 废弃错误、v1/v2 拒绝、浮点上下溢、类型默认值、send-snapshot 的无字段更新行为、墓碑拒绝和 include_tombstone 筛选。标签为任一匹配；engine=tikv 包含无 engine 标签的 TiKV，普通标签分支保留 Go 的 best-effort 写入语义。每个类型独立提交，成功持久化后才发布内存配置并以每分钟/60刷新真实限流器。

真实差分暴露并修复了注册 Store 后缺少限额配置的依赖：注册在资源锁内重试补齐，TiFlash 使用30/30默认值，已有条目不覆盖。bootstrap 本身保持 Go 的空映射，不提前创建条目。

进度复用 canonical Manager、Store 状态 worker 的转换前快照及同一 preparing threshold 算法。Manager 的独立120秒 GC 任务随宿主取消并 join；最后一个Manager释放时注销其指标，支持复用Registry和leader切换；巡检窗口从 canonical checker 的 patrol status 读取，没有已完成巡检时返回零时长。受控 Tokio 时钟测试证明真实巡检完成后30ms及取消后的零值。非有限进度保留 Go JSON 编码失败的文本500响应，不序列化为null。

## 行为证据

首次 `cargo test --locked -p astersql-pd-server --lib task_054_ -- --nocapture` 退出101：1项实际行为断言失败；缺失路由返回 `application/grpc`，不是编译失败。后续接线、测试准备和差分失败日志均保留，没有计为成功证据。

差分使用[只读监督入口](../../../scripts/http-store-limits-oracle.py)和已有 `bin/pd-server`，两端通过真实 tonic bootstrap/PutStore/region heartbeat 和 HTTP 运行相同场景。二进制 build info 未含 VCS revision，不宣称对应当前HEAD；外部etcd不冒称Go embedded-etcd拓扑等价。

46项两端观察包括：bootstrap空映射、参数错误及NumError数据、浮点溢出错误结构、单Store/全部Store、TiKV/TiFlash和标签OR、墓碑拒绝、v2拒绝、筛选未注册条目、进度按ID/动作、持久化重启。真实100→50 MiB region heartbeat推动下线进度0→0.5；两端观测速度5、剩余时间10秒。只对progress/current_speed/left_seconds的JSON数字表示统一为float64，不忽略字段、状态、错误、速度、ETA或持久化差异。

故障测试在真实etcd事务中注入不存在的lease，证明失败不发布配置；暂停实际配置事务后取消节点，检查500、shutdown完成及持久状态未变。follower写入转发、实际限流器rate=8/60、同MetaStore重启均有断言。progress包的受控时间、动态窗口、GC和指标测试一并回归。

## 验证

状态：已完成。以下均为通过结果；默认忽略项不计成功，Go差分另行显式执行。

动态原子领取共享槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。默认features，`CARGO_BUILD_JOBS=1 GOMAXPROCS=2 RUST_TEST_THREADS=4`；每次Rust修改后先运行 `cargo fmt --all`。

| 命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_054_ -- --nocapture` | 0 | 3通过、0失败、1忽略 |
| `cargo test --locked -p astersql-pd-schedule-checker --lib task_054_progress_ -- --nocapture` | 0 | 1通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-progress -- --nocapture` | 0 | 15通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 187通过、0失败、15忽略 |
| `cargo test --workspace --locked` | 0 | 2082通过、0失败、52忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-store-limits-oracle.py` | 0 | 1通过、0失败、0忽略；46项两端观察 |
| `cargo fmt --all` | 0 | 静态检查 |

workspace 回归曾暴露6项 Registry 重用/leader退任失败（AlreadyReg及其超时），已修复Manager指标注销；最终runtime独立回归54通过、0失败、36默认忽略，随后workspace完整通过。没有把失败归为无关环境问题。最后一次 `cargo fmt --all` 未产生任何变更，随后格式、workspace类型检查及聚焦行为验证均通过。

开始/结束Go守卫覆盖1101个文件（含ignored/untracked），存在性、SHA-256和Git状态完全一致；最终源码快照覆盖1258个Rust/Cargo文件。`plan.md`与基准HEAD逐字节一致。自己的槽位锁已释放，缓存保留，Cargo、测试、GC worker与oracle进程组均已结束。

原始红灯、失败与成功日志、确切命令/退出码、Go构建信息、46项观察及只读/源码快照：[task-054-evidence.json.gz](task-054-evidence.json.gz)。SHA-256 `325f237a505e2e5afa8c12f0dfa06b3206516c440f6a2af308633c0e3462eaa0`。

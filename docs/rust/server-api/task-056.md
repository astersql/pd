# 任务056：Region分页扫描HTTP

基准 `da9d80bdb`；依赖[任务055](task-055.md)与任务001的主题6准入记录已核验。Go、Go模块和 `plan.md` 保持只读。

## 来源与实现

- [source-inventory](source-inventory.md)：任务56及 `server/api/region.go` 的 GetRegions、ScanRegions、GetRegionCount、GetStoreRegions，`pkg/response/` 的 Region/RegionsInfo 编码行。复用 canonical RegionInfo、范围扫描及 response 编码，不复制领域判断。
- [wire-inventory](wire-inventory.md)：HTTP-084、085、087；任务明列的 count 对应 HTTP-086（原owner58），本次修正为56。HTTP-128删除全部缓存与分页扫描无关，按任务73及source-inventory的admin.go归属修正为73，未宣称实现。keyspace HTTP-088仍属78/主题8。
- [surface-inventory](surface-inventory.md)：router.go:258、261、262、263；审计和限流仍由81/82验收。
- [HTTP适配](../../../pkg/server/http_region_scan.rs)、[独立测试](../../../pkg/server/http_region_scan_test.rs)经现有同端口router、bootstrap、角色转发及request-context接线。补充 [canonical Store角色快照](../../../pkg/core/regions_shared.rs)与runtime读适配，以一次索引锁取得leader/follower/learner拼接或指定role树，避免HTTP层重写成员关系。follower复用已同步缓存及运行标记。

范围扫描保持包含起点所在Region、排除非空end_key及跨空洞行为；limit默认16，最大10240，非正数无上限。参数按Go首值、QueryUnescape、format=hex和Atoi处理。Store类型包括all/leader/follower/learner/witness/pending；负Store ID转换为uint64，缺失Store返回空列表。count使用索引计数并保留regions:null。列表响应为application/octet-stream，错误和count为JSON。

全量输出先取得不可变Arc快照，再以64项为一页按HTTP消费进度扫描、编码；count与输出属于同一快照，不跨多次可变缓存读取。保留跨页复用response对象的Go语义。此设计仍保留O(n)的Region引用快照，不声称O(1)内存或数据库游标。无后台producer，断开即丢弃未读页面；宿主取消在每项编码前检查，响应开始后以body错误结束流。初始索引快照与Go一样是一次同步读取。

## 行为证据

首次 `cargo test --locked -p astersql-pd-server --lib task_056_ -- --nocapture` 退出101：1项实际行为测试因新入口落入gRPC fallback、Content-Type错误而失败，非编译失败。

只读[监督入口](../../../scripts/http-region-scan-oracle.py)使用既有 `bin/pd-server`，两端经真实tonic bootstrap、heartbeat与TCP HTTP执行53项同场景观察。小集覆盖空洞、续页、hex/整数边界、参数解码、Store子树、未bootstrap和缺失数据；130 Region集合覆盖默认16条、全量/非正limit、Store列表、count及64+64+2续页，无重复或遗漏。比较完整状态/JSON及Content-Type；仅全量根map列表按ID排序，Store列表与范围查询保持原顺序。Go二进制无VCS revision，不宣称来自当前HEAD；外部etcd不代表embedded-etcd拓扑等价。

扩大差分时，Go在未知Store类型后接收新heartbeat发生curl超时（退出28，测试退出101）。只读源码 `pkg/core/region.go:GetStoreRegionsByTypeInSubTree` 的default分支在RUnlock前返回，与观察到的后续写入阻塞一致。保留原始失败日志，将该错误场景放到写入之后，仍比较错误响应，不模仿锁泄漏，不修改Go。

Rust额外覆盖follower转发/允许本地读取、真实etcd事务阻塞下的宿主取消、前后持久Region状态不变。流测试检查跨页对象复用、取消报错及未读Arc释放。TCP背压测试直接使用生产流编码与request-context中间件，在读取响应头后断开连接，等待真实请求token取消及未读Region释放；无mock调用次数断言。此流寿命测试不冒充额外MetaStore领域测试。

## 验证

动态原子领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。默认features，`CARGO_BUILD_JOBS=1 GOMAXPROCS=2 RUST_TEST_THREADS=4`。每次Rust修改后先运行 `cargo fmt --all`。

| 命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 静态检查 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-region-scan-oracle.py` | 0 | 1通过、0失败、0忽略；53项两端观察 |
| `cargo test --locked -p astersql-pd-server --lib task_056_ -- --nocapture` | 0 | 5通过、0失败、1忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 195通过、0失败、17忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2090通过、0失败、54忽略 |

状态：已完成。默认忽略项不计成功，本任务差分已独立显式运行。开始/结束Go守卫覆盖1101个文件（含ignored/untracked），存在性、SHA-256与Git状态完全一致；`plan.md`与基准HEAD逐字节一致。最终源码快照覆盖1260个Rust/Cargo文件。验证进程及服务已结束，自己的槽位1锁已释放，缓存保留。

原始红灯、Go锁泄漏场景失败日志、最终差分、命令/退出码、完整测试日志、Go构建信息、只读守卫及最终源码哈希：[task-056-evidence.json.gz](task-056-evidence.json.gz)。SHA-256 `bbb9ac73a46dbcf6a1f1281729f0655b4a6f56218308275dfc655a50b902ea32`。

# 任务055：Region点查询HTTP

基准 `cbecd9281`；依赖[任务054](task-054.md)及任务001的主题6准入记录已核验。Go、Go模块和 `plan.md` 保持只读。

## 来源与实现

- [source-inventory](source-inventory.md)：`server/api/region.go` 的 GetRegionByID、GetRegion、GetRegionSiblings，以及 `pkg/response/` 的 Region/Peer/list 编码来源行。直接复用已有 response crate，不复制领域对象或编码模型。盘点中的同文件慢列表、keyspace持久化行属于56–59/67；Store响应行已由51–54覆盖，不将它们误算成本任务新增能力。
- [wire-inventory](wire-inventory.md)：HTTP-060、082、083；任务明确要求的 siblings 对应 HTTP-109（清单原owner58），本次补充该行证据而不宣称58整体完成。
- [surface-inventory](surface-inventory.md)：router.go:221、253、254、286；这些行的审计接线仍属81，未宣称审计等价。
- [HTTP适配](../../../pkg/server/http_region_lookup.rs)、[独立测试](../../../pkg/server/http_region_lookup_test.rs)通过现有同端口路由、bootstrap/leader/request-context工作；[runtime邻接查询](../../../pkg/runtime/cluster_host.rs)在一次canonical cache读取中获取左右邻居。follower复用已有同步视图和运行标记。
- HTTP-060的标签查询通过现有term coordinator调用canonical RegionLabeler；只补查询命令，不实现70的规则管理HTTP。Rust从真实MetaStore中预存规则启动，Go通过已有规则HTTP写入，查询同一非空标签得到 `bar`。

ID查询保留ParseUint、零ID的空JSON对象400、缺失ID404；siblings使用Go的有符号Atoi后转uint64。key查询只QueryUnescape一次，保留加号、百分号、非UTF-8和NUL字节、hex大小写及错误。Gorilla父路由对解码路径的清理会使编码斜杠和点路径重定向；Location与空body均纳入差分。Region成功响应使用 `application/octet-stream`，错误和标签使用JSON类型。key缺失错误保留Go原始 `%v`。siblings复用Go兼容列表编码器，包含缺失左邻居的默认对象和缺失右邻居的对象复用，不擅自改成null。

## 行为证据

首次 `cargo test --locked -p astersql-pd-server --lib task_055_ -- --nocapture` 退出101：1项测试实际运行并因 `application/grpc` 与JSON响应类型不符失败，证明路由缺失；不是编译错误。真实Go差分进一步发现成功响应类型、key缺失错误和路径清理差异，均修正后重跑通过。

最终只读[监督入口](../../../scripts/http-region-lookup-oracle.py)使用既有 `bin/pd-server`，两端经真实tonic bootstrap、region heartbeat和TCP HTTP执行48项相同观察。比较完整状态/JSON/Content-Type以及重定向Location（仅剥离随机服务地址）。包括leader、epoch、down/pending/learner peer、大小字段、范围、缺失和邻接空洞、整数边界、编码key、hex、标签命中/缺失。Go二进制构建信息没有VCS revision，不宣称来自当前HEAD；外部etcd不代表Go embedded-etcd拓扑等价。

Rust额外验证follower自动转发和显式允许的同步本地读取。取消测试暂停真实Store标签事务，使Region GET等待共享资源；确认真实TCP无响应后取消宿主，既有外层中间件返回500 `server not started`，请求及宿主结束，实际etcd持久状态与取消前一致。并非mock调用次数测试。成功查询前后持久region记录也保持不变。response现有序列化/取消fixture测试随workspace回归。

## 验证

状态：已完成。动态原子领取共享槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。默认features，`CARGO_BUILD_JOBS=1 GOMAXPROCS=2 RUST_TEST_THREADS=4`；Rust修改后先运行 `cargo fmt --all`。

| 命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 静态检查 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-region-lookup-oracle.py` | 0 | 1通过、0失败、0忽略；48项两端观察 |
| `cargo test --locked -p astersql-pd-server --lib task_055_ -- --nocapture` | 0 | 3通过、0失败、1忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 190通过、0失败、16忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2085通过、0失败、53忽略 |

默认忽略项不计成功；本任务差分已独立显式运行。开始/结束Go守卫覆盖1101个文件（含ignored/untracked），存在性、SHA-256和Git状态一致。`plan.md`与基准HEAD逐字节一致；最终源码快照覆盖1258个Rust/Cargo文件。自己的槽位锁已释放，缓存保留；验证进程和服务已结束。

原始红灯、失败/成功日志、命令/退出码、Go构建信息、48项观察、Go守卫与最终源码哈希：[task-055-evidence.json.gz](task-055-evidence.json.gz)。SHA-256 `e8e229875bfc66ea33ac2ffe709bb726eb984f1fd9eab1686938731e2efd780c`。

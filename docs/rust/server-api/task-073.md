# 任务073：Region缓存管理HTTP

状态：已完成（仅任务文件限定的三个删除路由）。

基准 `812f7d69b`；依赖[任务072](task-072.md)和[任务004](task-004.md)的只读oracle已核验。Go、Go模块文件和plan.md保持只读。

## 来源与边界

[source-inventory.md](source-inventory.md):111为任务摘要；owner含73的来源行是237、317、497–499、674–713、2574、2704。237/497–499对应admin handler的缓存删除和持久删除；2574是admin.go原始哈希。317/674–713/2704是与大量RPC任务共享的grpc_service.go入口、worker、failpoint及哈希，不把这些共享行的其他RPC和故障点算作新增实现。

[wire-inventory.md](wire-inventory.md):258–260是HTTP-126、127、128三个DELETE路由。本任务文件的目的明确限定缓存及持久region删除；261的HTTP-129（persist-file）是较宽盘点留下的归属，不在这个删除边界内，保持未验收，不把SavePersistFile计作通过。[surface-inventory.md](surface-inventory.md)没有owner=73独立行；审计/限流等共享能力仍归原owner。

Go oracle为[admin.go](../../../server/api/admin.go)、[middleware.go](../../../server/api/middleware.go)和[server.go](../../../server/server.go)的ResetFollowerRegionCache。当前协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

[HTTP适配](../../../pkg/server/http_admin_cache.rs)复用ParseUint与JSON编码；在bootstrap admission之后验证参数。缓存单删允许不存在的ID；持久单删要求缓存中存在目标；持久删除失败仍返回200及Go错误正文，并保留缓存。成功删除持久元数据后才删除缓存。宿主[ClusterHost](../../../pkg/runtime/cluster_host.rs)复用唯一cluster、StorageEndpoint和leader TermStore事务围栏，不新增领域实现、存储副本或runtime。

[角色宿主](../../../pkg/runtime/member_role_worker.rs)在现有同步循环中串行处理follower reset，先丢弃旧同步future，再flush/删除其现有私有投影、将history index重置为0并重新连接；失败后也重连。全量删除扫描投影内的所有持久键，包含不在缓存中的键，不触碰leader元数据。现有follower投影是MemoryStore，本任务没有把它宣称为本地生产持久后端或Go LevelDB的durability替代。

HTTP非空pd-allow-follower-handle允许本地缓存重置；缺少该头由既有中间件转发leader；即使有该头，持久删除仍拒绝follower本地执行。region storage关闭时reset返回500；leader URL校验先于排队。

## 行为证据

[独立测试](../../../pkg/server/http_admin_cache_test.rs)在lib.rs显式注册，测试名前缀task_073_。初始失败命令：`cargo test --locked -p astersql-pd-server --lib task_073_ -- --nocapture`。可运行失败：未bootstrap的DELETE落入原兜底处理返回200，断言要求500；退出101，1失败、0忽略。开发期间的编译失败和错误测试装配另存原始输出，不算行为失败证据。

真实TCP HTTP、tonic及真实etcd覆盖：未bootstrap三个路由；非法/负数/溢出/零/MAX ID；缺失Region；缓存单删、全删、持久删除；删除后的HTTP404和RPC空region；持久快照及重启后恢复差异。实际etcd事务用无效lease拒绝整个删除事务，验证响应及内存/持久状态不变；取消在实际删除事务IO边界挂起，再shutdown，验证HTTP500、join及持久状态不变。这些故障不冒充Go failpoint差分。

[只读supervisor](../../../scripts/http-admin-cache-oracle.py)使用已有bin/pd-server，不构建Go服务、不执行Go测试或make/failpoint。双节点Go与双节点Rust比较follower单个/全量/缺失ID reset、非法ID、禁止本地持久删除、无头转发和同步恢复；leader数据保持可查。region-storage关闭场景在Go中通过共享配置后启动关闭该选项的follower验证；Rust通过已有配置入口读取同一选项验证。它不声称两者运行中配置传播时机相同。

Go本地RegionStorage会把心跳元数据延后批次写入；最初紧接心跳删除并重启时观察到已删除Region被旧批次重新写回。当前差分先等待5秒建立已落盘基线，再验收删除及重启；原始失败保留，不声称覆盖未flush心跳与删除的竞态，也不为适配此竞态另建Rust写缓冲。另两次扩展测试揭示Go follower运行中配置未更新、启动配置被共享配置覆盖；第三节点过早加入还曾被etcd unhealthy cluster拒绝，均保留原始失败，重启helper还因固定的默认name/initial-cluster覆盖传入参数而拒绝join。最终等待etcd健康窗口后启动关闭region storage的成员，取得同场景证据。

## 验证记录

首次workspace测试在既有`http_operator_read_test::task_062_behavior`失败：operator列表长度1，预期2；当轮server为247通过、1失败、34忽略，退出101。同一测试已在本次server包级全量回归通过，保持源码不变单独重跑又为1通过。随后以相同环境完整重跑workspace通过（结果见下表）；保留第一轮失败和窄重跑输出，不把偶发失败记成成功，也不声称已修复该既有测试的不稳定性。

实际动态领取共享槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。通过从1–10依次原子mkdir领取；锁记录计划、任务、会话和实际持有进程PID。所有Cargo及子进程继承该绝对路径，不使用--target-dir。Cargo回归阶段环境CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=1、GOMAXPROCS=2。修改Rust后先执行cargo fmt --all，再运行以下命令。

| 确切命令 | 退出码 | 有效测试数量 |
| --- | ---: | --- |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_073_ -- --nocapture` | 0 | 4通过、0失败、1忽略 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 249通过、0失败、34忽略 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2150通过、0失败、71忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-admin-cache-oracle.py` | 0 | 1通过、0失败、0忽略；28项响应观察及双方重启/follower同步 |

server包级包含248单元测试和1集成测试；0测试的doc-test不计行为证据。聚焦的1个忽略项是必须通过supervisor独立执行的Go差分，本次已显式运行通过。其他既有忽略项没有被声称为通过。

最终验证前后全部1320项Rust/Cargo输入SHA-256一致。全部1101项Go/模块文件（含ignored/untracked）的存在性、SHA-256及Git状态与开始一致；plan.md与开始字节相同。自己的slot-1锁已释放，Cargo编译缓存保留；清理了52个本任务已退出且无进程引用的临时fixture可执行文件（共1997522176字节），保留所有其他任务产物。实际服务、worker及Go差分进程组均已退出。任务文件已按要求标记完成后删除。

Go二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；build info为go1.26.8且无VCS revision，不声明来自当前HEAD。外部etcd不冒充Go embedded-etcd拓扑等价。

[压缩原始证据](task-073-evidence.json.gz)，SHA-256 `6335e1158464e1d7eb8f030535592b329db726ce760b34af534cb62b9f0c91bc`；包括初始可运行失败、开发期失败和通过、全部最终命令/输出/退出码/有效测试数量、Go构建信息、输入保护集合和清理记录。Python AST、Markdown链接、gzip/JSON往返及git diff --check另行核对。

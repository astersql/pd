# 任务074：恢复标记HTTP与RPC

状态：已完成。

基准 `9bd79ea38`；依赖[任务073](task-073.md)与[任务004](task-004.md)的只读oracle入口已核验。Go、模块文件和plan.md只读；协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 来源与实现边界

[source-inventory.md](source-inventory.md):112是任务摘要；owner含74的来源行为237、317、497–499、674–713及哈希行2574、2704。237/317共享整个Go文件，497–499属于前序缓存删除，674–713含其他RPC/worker/failpoint；本任务只验收恢复标记，不把这些共享来源的其他行为算作新增覆盖。[wire-inventory.md](wire-inventory.md):23对应RPC-IsSnapshotRecovering，262–267对应HTTP-130至HTTP-135六条路由。[surface-inventory.md](surface-inventory.md)没有独立owner=74行；4856–4861的共享审计仍归81。

只读oracle为[admin.go](../../../server/api/admin.go)、[server.go](../../../server/server.go)的六个标记方法和[grpc_service.go](../../../server/grpc_service.go)的IsSnapshotRecovering。GET检查精确键的存在性，空值也算已标记；POST使用CreateRevision=0条件事务写入`on`且保留已有值及版本；DELETE允许键不存在。HTTP不依赖bootstrap、不解析未使用的body/query；读结果为marked对象，修改成功为空正文。RPC直接读取共享权威状态，不检查请求cluster ID、不要求leader或bootstrap；存储读取错误放入UNKNOWN错误header。

[HTTP及共享标记操作](../../../pkg/server/http_admin_markers.rs)复用既有ClusterPaths及backend-neutral MetaStore，不复制领域模型、不新建存储副本或runtime。[RoleServices](../../../pkg/runtime/member_role_worker.rs)通过已有共享store提供权威存储，独立于active leader term。[Grpc](../../../pkg/server/grpc.rs)实现RPC，[SyncGrpc](../../../pkg/server/sync_transport.rs)补实际listener使用的外层转发；[HTTP装配](../../../pkg/server/http.rs)挂接六条路由。既有[待实现RPC检查](../../../pkg/server/grpc_unsupported_test.rs)移除owner74的未实现断言，保留其他owner。

## 行为与差分

[独立测试](../../../pkg/server/http_admin_markers_test.rs)在lib.rs显式注册，全部task_074_前缀。初始可运行失败为旧HTTP兜底空正文导致JSON EOF，退出101、1失败、0忽略；恢复空间后的真实网络测试进一步捕获缺少SyncGrpc转发，RPC返回Unimplemented，0通过、2失败、1忽略。后续修复测试重启时未复用已持久成员URL造成的IdentityConflict。以上失败原文保留，不将编译/磁盘错误算作行为失败。

最终测试使用真实TCP HTTP、tonic和真实etcd，覆盖未bootstrap、错误请求头ID仍可读取、follower读取/修改、重复设置/删除、create/mod revision及值不变、已有空值、相邻前缀键不串读、两个标记重启恢复及删除后重启。事务故障由实际事务追加不存在lease的写入，使etcd拒绝整个事务，核对HTTP500与持久快照不变；读取故障由实际etcd未来revision请求触发，核对HTTP500及RPC UNKNOWN header。实际读/删除IO边界暂停后shutdown，验证HTTP取消、RPC取消header、worker退出及持久标记保持；没有以mock调用次数验收。

[只读supervisor](../../../scripts/http-admin-markers-oracle.py)使用已有Go二进制，关闭Go模块/工具链下载、不构建Go服务、不执行Go测试、make或failpoint注入，复用共享Go守卫及进程组清理。双实现比较十组HTTP状态/原始正文/Content-Type和RPC读值；GET未使用body/query、重复设置、无键删除、错误请求cluster ID均通过。双方另验证重启时标记仍存在，删除并再次重启时不存在；Go真实第二成员和Rust真实第二节点验证follower读取及共享标记修改。第一次差分捕获Go JSON正文末尾换行而Rust没有，已局部修正标记响应编码，最终原始正文一致（仅Content-Type大小写转为小写比较）。

故障注入不是Go failpoint差分，不声称其底层错误字符串与Go etcd-client一致。Rust使用外部etcd，Go已有二进制使用embedded-etcd；不把二者拓扑、etcd进程崩溃恢复或本地生产后端durability宣称等价。

## 空间恢复与槽位

最初可用不足200MiB，真实etcd无法创建WAL；重试rustc及测试check均报No space left on device。先清理本任务已退出fixture临时二进制和失败构建未完成working目录；随后检查target、/private/tmp及当前用户系统T目录，未发现可安全删除的大型临时集群；没有删除其他项目临时目录或共享Cargo缓存。

最终在独占槽位内对307个单硬链接Cargo对象文件执行`ditto --hfsCompression --noclone <source> <temporary>`，逐项核对内容SHA-256和大小后原子替换。APFS透明压缩保留原路径、内容及可读取性，回收5,369,499,648字节，空间恢复约5GiB；有效缓存全部保留。逐文件哈希与回收量随原始证据保存。

实际动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。每阶段从1–10原子mkdir领取，owner记录计划、任务、session及持有shell PID，所有Cargo/fixture/差分子进程继承该绝对路径，不使用--target-dir。验证环境CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=1、GOMAXPROCS=2，默认features；Rust修改后先cargo fmt --all。

## 最终验证

| 确切命令 | 退出码 | 有效测试数量 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_074_ -- --nocapture` | 0 | 2通过、0失败、1忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-admin-markers-oracle.py` | 0 | 1通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 251通过、0失败、35忽略 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2152通过、0失败、72忽略 |

server包级包括250个单元测试及1个集成测试；0测试doc-test不算行为证据。默认忽略的本任务Go差分已单独显式执行通过，其他既有忽略项不宣称通过。

最终1312项Rust/Cargo输入在完整回归前后SHA-256一致。全部1101项Go/模块文件（含ignored/untracked）的存在性、SHA-256及Git状态与开始完全一致，plan.md字节相同。服务、worker及Go差分进程组均已退出；确认PID已退出且lsof无打开引用后，清理31个临时fixture可执行文件，共1,190,830,528字节。自己的slot-1锁已释放，编译缓存保留。任务文件已标记完成后按要求删除，原有任务58删除及prompt.md变更保留在提交之外。

Go二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；build info为go1.26.8且无VCS revision，不声明其来自当前HEAD。

[压缩原始证据](task-074-evidence.json.gz)，SHA-256 `650e309e62e8be41631bd8071658015036207d9e63ca0664ee0f8fc79ff80bb0`；包含初始失败、开发期失败、最终完整命令/输出/退出码及测试计数、Go构建信息与监督结果、Go及Rust输入保护集合、逐文件压缩哈希/空间回收记录及临时fixture清理结果。Python AST、Markdown新增链接、gzip/JSON往返和git diff --check另行核对。

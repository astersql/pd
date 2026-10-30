# 任务046：集群状态与版本 API

状态：已完成（沿用现有majority复制模式能力边界）。

基准：`6cd6f6d37`。任务045的真实成员/leader管理证据已核对。Go、Go模块文件及plan.md只读。

## 实现和来源

[HTTP适配](../../../pkg/server/http_cluster.rs)接通GET `/pd/api/v1/cluster`、ANY `/pd/api/v1/cluster/status`、GET `/pd/api/v1/status`和GET `/pd/api/v1/version`。复用现有同监听器router、请求取消及leader转发。

[宿主观测](../../../pkg/runtime/cluster_host.rs)读取当前任期配置与canonical region索引。`max_peer_count`来自当前replication配置，不能用raft元数据中的旧值替代；初始化判定与Go一致：region数量大于1，或包含空起始key的region具有足够voter且没有pending peer。观测复用region索引，不复制全量region。bootstrap时间通过同一MetaStore线性一致读取，使用已有typeutil解码和chrono输出RFC3339Nano（去除尾零）。空值输出Go零时间，损坏字节返回HTTP500和完整ErrBytesToUint64错误前缀。

`/status`报告router/监听服务启动时的Unix秒数、Classic内核及Rust构建信息；`PD_RELEASE_VERSION`编译环境可提供发布版本，默认真实Cargo package版本；`PD_BUILD_TS`、`PD_GIT_HASH`未提供时显式为`None`。`/version`保留Go实际返回的空build_time/hash/branch字段，version与status一致。未把Go oracle二进制的版本冒充Rust构建版本。

复制状态沿用现有bootstrap的majority能力边界：Go gogo CompactTextString会省略默认MAJORITY枚举，实际字符串为空；bootstrap前亦为空。已有运行时拒绝无manager的DR bootstrap；若恢复的配置要求DR状态，本接口明确报错，不虚构DR manager状态。本任务不宣称完成DR manager或embedded-etcd拓扑迁移。

来源映射：

- [source-inventory](source-inventory.md)的任务46行、cluster.go/status.go/version.go函数及指纹行；实际实现额外对照server/server.go GetCluster/GetClusterStatus与server/cluster/cluster.go LoadClusterStatus/isInitialized。
- [wire-inventory](wire-inventory.md) HTTP-019、020、116、117全部由当前实现覆盖。HTTP-094 `/regions/version`属于region epoch排序，按任务57“regions各top维度”修正owner为57并保留其未验收标记；没有把该路由计入本任务通过。
- [surface-inventory](surface-inventory.md) router.go 167、168、294、295注册行。审计接线仍归任务81，本任务不宣称审计完成。

## 失败与修复证据

首次真实TCP测试编译运行成功后失败于行为断言：`/cluster`空body被解析为null，期望真实cluster id与max_peer_count，退出101、1失败。

Go运行差分随后暴露两个真实兼容问题：majority protobuf文本为`""`而非`"mode:MAJORITY "`；损坏时间错误带`[PD:typeutil:ErrBytesToUint64]`前缀。两次失败均保留，修正后完整比较通过。

测试修正也保留记录：新增chrono及compat dev依赖后先通过offline检查更新锁，再恢复locked验证；Rust字段访问编译错误不作为行为失败证据。真实etcd future revision错误经既有存储映射为`revision is newer than the store`；宿主取消由外层HTTP redirect中间件返回500纯文本ErrServerNotStarted，测试按既有公共契约核验。Go心跳流等待第一条请求后才返回header，测试改用并发发送/接收并显式取消join接收任务；曾阻塞的一轮停止自有Go进程后正常回收。重启测试最初改变成员URL触发IdentityConflict，最终复用同一地址，未放松身份约束。

## 验证场景

[行为测试](../../../pkg/server/http_cluster_test.rs)：真实TCP HTTP与tonic Bootstrap/RegionHeartbeat、同一etcd MetaStore。验证bootstrap前零时间和false、持久时间与HTTP解析时间相同、3个voter与pending切换true/false/true、read前后bootstrap条目不变、损坏时间500且恢复后继续服务；status字段类型/集合、启动时间范围和跨请求稳定性、version一致性。另验证配置max-replicas=5与raft元数据max_peer_count=9时HTTP返回5，两个region直接初始化，两个成员的leader/follower访问一致，重启两次保持时间及状态。

[故障测试](../../../pkg/server/http_cluster_fault_test.rs)：只在真实MetaStore bootstrap-time读取边界设gate，future revision由真实etcd拒绝；在读取挂起时取消宿主，收到既有500 ErrServerNotStarted，node.shutdown在5秒内join完成。错误/取消前后raft条目不变，没有以mock次数代替状态验证。

[只读差分入口](../../../scripts/http-cluster-oracle.py)启动既有`bin/pd-server`，对Go与Rust执行同一场景；完整比较bootstrap前JSON、majority状态、pending初始化序列、损坏时间完整错误及字段集合。分别核验各自实际cluster id、bootstrap持久时间、版本内部一致性与启动秒数；随机身份、时间原点和不同构建版本不要求数值相同。Go binary build info没有VCS revision，不声称它来自当前HEAD。差分为显式运行的1项测试，默认ignored不计通过。

## 验证记录

全部阶段依次原子尝试共享槽位1–10，实际均为槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁内记录计划、任务、会话和持有进程PID；Cargo及子进程继承该绝对路径，缓存保留。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | server静态检查 |
| `cargo check --workspace --locked` | 0 | workspace静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_046_ -- --nocapture` | 0 | 3通过/0失败/1默认忽略 |
| `RUST_TEST_THREADS=4 cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 167通过/0失败/7默认忽略，含integration target |

独立差分命令（同一槽位环境）：

```sh
PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-cluster-oracle.py
```

退出0，内部执行`cargo test --locked -p astersql-pd-server --lib task_046_differential -- --ignored --nocapture`，1通过/0失败/0忽略。实际新启动Go/Rust节点，并非回放历史Rust结果。最终通过记录为`target/rust-slot-1/http-cluster-oracle-1793326247415296000.json`；Go binary SHA-256、build info、所有失败与成功输出及只读守卫随证据归档。

工作区首轮 `RUST_TEST_THREADS=4 cargo test --workspace --locked` 退出101：2019通过/1失败/44默认忽略，失败为未改动的syncutil `singleflight_randomized`首次poll必须pending的并发断言。未修改该模块；单独 `cargo test --locked -p astersql-pd-syncutil --lib singleflight_randomized -- --nocapture` 退出0、1通过。随后完整 `RUST_TEST_THREADS=4 cargo test --workspace --locked --no-fail-fast` 退出0：2056通过/0失败/44默认忽略，含compile-fail doctest。保留首轮失败，不把复核描述为首次即通过。

最终1101个Go/模块文件（含ignored/untracked）的路径集合、SHA-256与受保护Git状态均与开始相同；plan.md哈希不变。最终验证期间1226个Rust/Cargo文件哈希不变。Python语法、文档链接、`git diff --check`通过。无自有测试/oracle进程及槽位锁遗留，编译缓存保留。

[压缩原始证据](task-046-evidence.json.gz)保存确切命令、退出码、有效测试数量、所有失败与成功日志、Go binary指纹/build info、Go只读守卫与Rust/Cargo指纹。SHA-256：`6a8f073ef9aabb7bfb0b7ce09659a5cf69ed0a255eb4c5971bae150ecfb899b2`。

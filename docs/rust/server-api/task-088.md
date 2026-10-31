# 任务088：成员加入预检

状态：已完成。

基准 `26e6771d4`；2026-11-01。依赖[任务087](task-087.md)及任务1–4的清单和只读oracle入口已核验；协议仍固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。原始命令、Go进程日志、只读快照和owner88原始来源行保存在[压缩证据](task-088-evidence.json.gz)。

## 行为与接线

[预检](../../../pkg/runtime/join_preflight.rs)由现有[PdNodeBuilder](../../../pkg/runtime/node.rs)的`with_join`接入，先于diagnostics/admin绑定、Member::bootstrap、选主和worker写入。空join沿用bootstrap/restart；非空join代表对显式指定外部MetaStore的全新成员准入。检查原始join自连接、异常未启动成员、重复名称、已有member ID，以及其他peer身份占用的advertise URL。自连接使用原始配置字符串；URL占用检查使用解析并分配端口后的最终advertise地址。客户端和peer URL规范化复用既有Endpoint，覆盖大小写、DNS尾点、十进制端口和IPv6；peer集合相同的条目不作“其他成员”URL冲突。损坏的成员记录、无匹配注册目标、无效endpoint及后端读取失败均拒绝准入。

外部MetaStore的member列表使用同一真实etcd的线性一致快照；join必须匹配该快照中已发布的PD客户端URL，防止把另一个join目标误当作当前MetaStore的成员。此限制是外部后端的显式适配条件，不声称Go客户端会做这项registry匹配。预检没有MemberAdd、URL预留事务或持久副作用；与Go新成员检查一样，不能保证两个并发join在发布前的URL唯一性。

为使独立入口使用同一行为，局部扩大到[CLI](../../../cmd/pd-server/main.rs)和[独立CLI测试](../../../cmd/pd-server/main_test.rs)：解析并传递`--join`；非空join在bootstrap_config之前要求已有`/pd/cluster_id`，避免空后端被误建为新集群。`--initial-cluster`仍是既有兼容harness元数据，不用于生成Go embedded-etcd拓扑。

## 真实验证与边界

[独立Rust测试](../../../pkg/runtime/join_preflight_test.rs)通过公开builder启动真实监听器和同一外部etcd MetaStore；成功后检查GetMembers RPC、HTTP ping及持久member记录，shutdown后以无join方式复用原身份重启。错误和取消检查完整`/pd/`条目（含revision）不变，并验证失败监听端口可重绑。停止真实etcd导致读取失败；重新启动后核对原持久状态。预取消明确返回node is stopped。没有以mock次数或零测试作为验收。

[只读Go supervisor](../../../scripts/join-preflight-oracle.py)使用既有`bin/pd-server`，SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，go1.26.8/without_dashboard/darwin-amd64；二进制无VCS revision，不声称来自当前HEAD。运行Go自连接、重复名称、重复advertise URL、规范化端口重复URL和真实成功加入，对照Rust相同语义的拒绝与双成员结果。Go拒绝后成员列表原样不变，成功后2成员。Go端的HTTP成员列表检查与Rust的RPC/HTTP和持久状态断言分别保留，没有把etcd member ID与Rust member ID归一成同一个身份。

Go带数据目录的restart携带join可以成功，由实际Go进程验证并单列记录；Rust不读取或伪造该目录，外部MetaStore已有身份用无join方式重启。Go embedded-etcd的MemberAdd/非线性列表恢复、未启动peer恢复及数据目录语义属于任务89/主题10，不宣称本任务实现其拓扑或持久化等价性。已存在完整名称的fresh join必须拒绝。外部MetaStore拥有已发布PD身份，Go拥有embedded-etcd成员身份，两者仅比较预检语义。

## 来源映射

[source-inventory](source-inventory.md)有10条owner88行（含共享owner）：任务88映射；memberutil/member.go的公开GenerateUniqueID及manifest；client_url.go内部实现及manifest；join.go公开PrepareJoinCluster、3条failpoint和manifest。原始行完整归档。wire-inventory与surface-inventory无独立owner88行。

`join.go`的无join、自连接、fresh重复名称和client URL前置检查由运行差分覆盖。`client_url.go`的规范化由真实端口差分及Rust DNS/IPv6/peer排除边界补充；已有Endpoint对路径/凭据等非transport输入更严格，沿用任务39约束，不把Go对任意坏URL的回退行为算为支持。3条`addMemberFailed` failpoint属于预检后的MemberAdd/重试，不运行Go注入，不在本任务复制，移交89及主题10；本次后端真实读取故障覆盖预检自身的失败边界。

`GenerateUniqueID`的Go实现是SHA256前8字节小端，但只被TSO keyspace-group和独立MCS调用（只读rg核对）；classic join不调用它。外部MetaStore继续使用现有bootstrap_config身份及Member::bootstrap冲突检查，不改成不同算法、不声称ID字节等价，也不移植范围外MCS入口。

## 失败与验证记录

先装配最小接口并编译可运行测试，实际失败命令退出101：self join被接受，fresh join复用已有名称也被接受，0通过/2失败；不是编译错误。实现后2项通过，补边界后3项通过。随后动态端口测试证明配置`:0`漏查实际分配URL（退出101，0通过/1失败）；宿主将原始self/peer配置和最终client advertisement分别传入同一预检后，聚焦集合扩为4项。曾新增“后端停机后延迟30ms取消”的断言，实际后端不可用先返回，2通过/1失败；已去掉不确定的时序假设，保留独立真实后端错误与预取消断言，没有放宽生产错误处理。

所有Cargo及调用Cargo的supervisor阶段逐次原子动态领取slot-1，绝对`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；默认features，`CARGO_BUILD_JOBS=4`；串行阶段`RUST_TEST_THREADS=1`，最终runtime与workspace回归用命令显式设为4。每阶段由持锁进程记录任务、会话与PID，退出后释放自己的锁，保留缓存。下表和最终只读/清理记录由原始日志核对填写。

| 确切命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-runtime --lib task_088_ -- --nocapture`（TDD行为失败） | 101 | 0通过、2失败 |
| 同一聚焦命令（最终） | 0 | 4通过、0忽略 |
| `env RUST_TEST_THREADS=4 cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 68通过、36项既有忽略 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --nocapture` | 0 | 6通过、0忽略（lib 5，真实进程 1） |
| `cargo fmt --all`；`cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据检查 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 编译检查 |
| `cargo check --workspace --locked` | 0 | 编译检查 |
| `cargo test --locked -p astersql-pd-runtime --lib task_088_assigned_port_conflict -- --nocapture`（分配端口行为失败） | 101 | 0通过、1失败 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/join-preflight-oracle.py` | 0 | Rust 4通过、0忽略；Go 4组拒绝、1组成功加入、1组带数据重启 |
| `cargo test --workspace --locked`（动态端口修正前，串行） | 0 | 2196通过、76项既有忽略 |
| `env RUST_TEST_THREADS=4 cargo test --workspace --locked`（最终） | 0 | 2197通过、76项既有忽略 |

最终Go只读守卫：开始/结束1101个Go/模块文件（含ignored/untracked）的存在性、SHA-256及Git状态完全相同；plan.md原字节与基准一致。未运行make、failpoint启停或Go写入入口。所有验证进程及supervisor子进程已结束，自有slot-1锁与owner记录已删除，缓存保留。用户已有prompt.md变更原样保留且不纳入提交。

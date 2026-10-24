# 任务49：桶报告协议适配

依赖task-48.md已完成，累计前置沿其证据；已读两份只读plan、根规则、Copilot、ADR、依赖基线和移植指南，无更深AGENTS。
起点commit、Go/模块逐项内容和状态、plan哈希见[before.json](task49/before.json)。

## 实施前oracle

Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0。审查server/grpc_service.go:1090–1270、
server/cluster/cluster_worker.go:277、cluster.go:1310及pkg/statistics/buckets/hot_bucket_cache.go。
复用task47既有native pd-oracle47，未编译或改写Go；实际运行[oracle.py](task49/oracle.py)，
Rust真实tonic探针为pkg/server/examples/bucket_heartbeat_oracle.rs，[oracle.out](task49/oracle.out)。
region1/peer2/store1，报告keys=[空,m,空]、period=2000ms，六数组完整传入。
missing99、v3、v2、重复v3均500ms无逐条ack；EOF返回Unknown/EOF，wrong cluster为FailedPrecondition。
HTTP热点快照确认missing无热点，接纳后region1两桶边界/每秒负载正确，旧版和重复仍增加hot_degree。
GetRegionByID虽请求need_buckets，但Go默认store bucket feature关闭，不暴露report，不冒称该接口验证版本。
版本/并发CAS/Removed实际运行快照复用[task-45.md](task-45.md)及其原始oracle输出。
Go与Rust共享毫秒period、原始六类counter；worker换算每秒，QPS与keys保持各自字段。
源码补充契约：nil/空keys忽略；缺失region领域错误被流吞掉继续；Removed不入worker；
低/同版本CAS返回applied=true且不替换report metadata，但仍交worker；不得在协议层额外过滤。
输入数组按Go领域前提提供完整长度；本任务不重写worker算法或新增容错定义。

## 红灯与实现

[red.out](task49/red.out)先执行cargo fmt --all与cargo check -p astersql-pd-server --offline，
后者仅维护已有workspace dependency边的Cargo.lock。最小接口已编译，真实HTTP/2 loopback测试
收到Unimplemented而期望Go实测Unknown/EOF；1项执行、0通过/1失败/0忽略，退出101。
不是缺符号或零测试。listener和真实bucket worker在断言前已cancel/join。
[green.out](task49/green.out)首轮6通过；完整验证再增加同一流继续接收的第7项。

新增BucketHeartbeatService与bucket_from_proto；PdServices默认可选入口，Grpc::report_buckets
调用宿主真实cluster admission。缺失region错误吞掉继续，nil/空keys不入领域；接纳和热点提交
复用handle_bucket_heartbeat，不在协议层重新计算applied或过滤旧版。默认宿主不提供服务返回
Unimplemented；bootstrap错误响应保留cluster ID，错误类型NotBootstrapped。
EOF的Unknown/EOF源于Go bucketHeartbeatServer.recv对io.EOF做errors.WithStack，
直接`err == io.EOF`不成立，最终gRPC映射Unknown/EOF；Rust保留实际可观察结果。

7项真实tonic loopback行为测试覆盖wire encode/decode、全部六counter数组、毫秒period、
worker每秒速率、零period默认10000ms、缺stats/单key零桶、缺region、nil/空keys、旧/同版本、
Removed不入队、并发流最终version最大且worker保留最后接纳负载；region_epoch999不额外拦截。
同一流依次missing99/nil/空/v3/v2，正常继续，metadata为v3而热点使用v2负载；未知store允许。
宿主取消与客户端断开均以active future归零、无报告提交、listener join验证；worker显式shutdown。
开流不关闭时无ack，已应用的core/热点状态可见。

生产库不创建runtime、任务或Registry，不引入protobuf到cluster/statistics领域；
不新增调度、转发、速率控制或持久化。本任务仅增加已有workspace statistics-buckets依赖边，
无新第三方包或feature。相邻.rs.md说明行为和宿主装配责任。
本路由无MetaStore I/O，workspace后端契约只作为回归，不冒称新增路由持久化能力。
EOF、空流和取消均终止本RPC；无脱离流生命周期的接纳future。

## 最终验证与资源退出

统一入口：`bash docs/rust/state/task49/slot.sh python3 docs/rust/state/task49/validate.py`。
[summary.json](task49/summary.json)记录确切命令、退出码和有效测试数量，
[validation.out](task49/validation.out)与validation-0.out至validation-6.out保留输出。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 成功 |
| `cargo test -p astersql-pd-server --locked --lib state_task_49_bucket_heartbeat_adapter -- --nocapture` | 0 | 7通过/0失败/0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 32通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 成功 |
| `cargo fmt --all -- --check` | 0 | 成功 |
| `cargo check --workspace --locked` | 0 | 成功 |
| `cargo test --workspace --locked` | 0 | 1286通过/0失败/1忽略 |

workspace既有1项ignored为tso_processes，本任务无忽略。真实MemoryStore/EtcdMetaStore契约均通过。
已审查pkg/storage/kv/etcd_fixture_test.rs与scripts/rust-etcd-fixture/main.go：仅构建已有Go源码，
继承GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0；无生成/注入，EOF关闭并wait，
失败时kill后wait。proto/build.rs校验固定ZIP后只向OUT_DIR生成Rust。
本任务没有运行Go测试、make/failpoint启停或任何Go生成/格式化/整理入口。

全部oracle/红灯/绿灯/最终验证阶段动态领取槽位2，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
[resources.json](task49/resources.json)确认阶段子进程全部退出后自有锁已释放、缓存保留，
没有使用槽位产物的进程或oracle49数据目录。Go服务TERM退出0并wait；
loopback listener与真实worker均cancel/join，客户端断开与宿主取消时active future归零。
复用Go二进制SHA256为446a9ce18ed5ac513d4cc66306cfe276db44cd2198db2d9e4e53d20520040523。

[readonly.json](task49/readonly.json)确认1101项Go/模块的集合、逐项内容及Git状态与起点相同，
聚合SHA256为ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4；
两份指定plan.md哈希相同，Go参考文件与固定基准无差异。
workspace fmt未产生范围外Rust改动。证据日志仅规范制表符和行尾空白；
git diff --check与证据Markdown链接检查通过，用户既有计划变更排除提交。

状态：已完成。仅声明桶报告协议状态适配完成。

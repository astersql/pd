# 任务072：Affinity更新HTTP

状态：已完成。基准 `77cf8432e`，依赖[任务071](task-071.md)及[任务004](task-004.md)只读oracle已核验。Go/模块文件与plan.md保持只读。

## 来源与实现

[source-inventory.md](source-inventory.md):110为摘要；owner含72的全部来源行是270、530–531、1341–1387、2614，分别对应v2 affinity handlers、删除持久化入口及既有canonical affinity实现/测试。共享领域和调度行为复用原crate，不把其调度、split、缓存及其他owner的行为算作本任务新增验收。[wire-inventory.md](wire-inventory.md):305–308为HTTP-173至176的POST/PATCH/PUT/DELETE；只更新这四行及来源摘要。[surface-inventory.md](surface-inventory.md)没有owner=72独立行；共用安全、审计、限流仍由原owner负责。

[HTTP envelope](../../../pkg/server/http_affinity_write.rs)与[共享router](../../../pkg/server/http_affinity_read.rs)接入既有PdServices和term命令队列。[宿主适配](../../../pkg/runtime/affinity_write_http.rs)在原leader/bootstrap admission锁内，借用宿主唯一持有的Manager及RegionLabeler，调用既有create/delete/update ranges/update peers事务API。没有新增manager、worker、存储副本、私有runtime或领域实现。runtime增加workspace已有base64和keyutil依赖，锁文件仅补对应依赖边，不改变版本或features。

Go oracle是`server/apiv2/handlers/affinity.go`及`pkg/schedule/affinity/`。POST支持创建、skip_exist_check和按delete参数存在性切换批量删除；DELETE支持force；PATCH在提交前校验并合并同组操作，拒绝同时add/remove；PUT复用manager检查leader/voters和store存在性。请求bytes按Go的base64或字节数组解码，持久label边界保留canonical hex编码。非法range、group、重复peer及缺失store不部分修改内存或持久状态。存储批次仍遵守已有manager的MAX_BATCH_OPS策略，不扩张为超大跨批次事务保证。

Gin GetQuery对空值仍返回存在，`?delete`必须执行批量删除；空skip_exist_check须ParseBool失败。BindJSON先写400，Content-Type保留`text/plain; charset=utf-8`，正文仍为JSON字符串；ShouldBindQuery的DELETE错误保留JSON Content-Type。现有ErrBindJSON的cause重复封装按实际Go输出保留。这些差异均通过运行差分发现并修正，不以源码推导代替运行证据。

## 行为与差分证据

[独立测试](../../../pkg/server/http_affinity_write_test.rs)在lib.rs显式注册，全部使用task_072_前缀。初始可运行失败为POST返回405而非bootstrap的500，退出101，1失败、0忽略；另一次最早尝试在编译阶段磁盘耗尽，0个有效测试，不算TDD行为证据。

最终聚焦4通过、0失败、1忽略；忽略项是必须通过[只读supervisor](../../../scripts/http-affinity-write-oracle.py)单独运行的Go差分，本次已显式运行1通过、0忽略。真实TCP HTTP、tonic bootstrap、真实etcd和相同宿主Manager覆盖：成功增删、skip、range冲突、半空/逆序边界、非法/不存在group、PATCH冲突/合并/精确删除、peer必填/重复/非voter leader/缺失store，以及非法base64、JSON字段类型、空体/截断JSON。每个错误场景同时核对完整GET状态和持久group/label快照不变。

事务故障测试在实际MetaStore事务边界追加不可能满足的create_revision条件，由真实etcd拒绝提交，覆盖create/PATCH/PUT/DELETE且内存和持久状态不变；解除故障后创建成功。取消测试仅挂起同一事务提交前的IO边界，取消宿主后HTTP500、shutdown完成join，持久快照不变。故障/取消不冒充Go failpoint差分，也不用mock调用次数验收。

Go差分复用现有bin/pd-server，不重建Go服务器、不运行Go测试/make/failpoint。双方执行42项相同观察，包括HTTP状态、完整JSON错误/响应、Content-Type和每次写后的group/label持久内容；再通过双方真实重启恢复带00/FF二进制边界和peer分布的group，核对重启前后完整持久JSON值相等、恢复后的HTTP状态及删除副作用。额外Rust测试通过follower转发真实POST/PUT，再重启leader检查恢复。

仅在跨实现比较中把动态create_timestamp归一化为标记，归一化前断言其为正Unix秒；本实现重启前后的原始时间和持久JSON值不归一化。错误、phase、range_count、leader/voters、hex和HTTP状态原值比较。Go二进制没有VCS revision，不声称来自当前HEAD；外部etcd也不宣称等价embedded-etcd拓扑。

## 磁盘恢复与槽位

实际动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。原子mkdir领取，记录任务、会话和持有进程；所有Cargo/fixture/差分子进程继承该路径。验证环境CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=1、GOMAXPROCS=2。

首次仅117MiB可用。先清理本次失败构建的未完成产物；继续执行时确认8个etcd-contract/pd-compat临时集群的原持有PID均不存在、进程命令无引用、目录不含Go/模块文件后清理，空间恢复至1.3GiB。完整测试重新编译仍耗尽空间，又检查到槽位根目录649个按PID生成且持有进程已退出的rust-etcd-fixture临时二进制（逻辑24,930,613,312字节）；清理后可用23GiB，后续验证继续。没有删除共享Cargo有效缓存、用户临时工作目录或Go/模块文件。原始输出保留磁盘失败、错误测试假设（非精确range删除预期误设200，按领域/Go改为404）、开发期差分失败及最终结果，不把它们计为通过。

## 最终验证

| 确切命令 | 退出码 | 有效测试数量 |
| --- | ---: | --- |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_072_ -- --nocapture` | 0 | 4通过、0失败、1忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 245通过、0失败、33忽略 |
| `cargo test --locked -p astersql-pd-schedule-affinity -- --nocapture` | 0 | 44通过、0失败、0忽略 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2146通过、0失败、70忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-affinity-write-oracle.py` | 0 | 1通过、0失败、0忽略；42项观察及持久快照/双方重启 |

server包级包括244单元测试和1集成测试；0测试doc-test不计作行为证据。所有Rust/Cargo输入共1307项在整轮验证前后SHA-256相同，并与最终Go差分的相同输入哈希一致。全部1101项Go/模块文件（含ignored/untracked）的存在性及SHA-256与开始一致，Go Git状态无差异；plan.md与HEAD原字节相同。进程/worker均完成退出，自己的slot-1记录和锁已清理，缓存保留。Python AST、Markdown新增链接、gzip/JSON往返及git diff --check另行核对。

[压缩原始证据](task-072-evidence.json.gz)，SHA-256 `f9ab09758f01d9ccc44106f692f33e4502af627b465dd155f9183192ebc8b8c1`。包含初始行为失败、开发期输出、最终完整命令/退出码/测试数量、Go构建信息、保护集合及清理结果。Go二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`；没有VCS revision，因此不声明其构建来源为当前HEAD。

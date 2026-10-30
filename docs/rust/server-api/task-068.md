# 任务068：Placement单规则HTTP

基准 `423e19be8`；依赖[任务067](task-067.md)与任务001–004已核验。默认features，kvproto固定 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。Go与模块文件、plan.md只读。

## 来源与实现

[source-inventory.md](source-inventory.md):106、259、518、1733–1781的placement扫描来源及2601；[wire-inventory.md](wire-inventory.md):170、171、173–176，HTTP-038/039/041/042/043/044。按任务68明确的group/key/region读取要求，前三项从原任务69盘点归属移至68；region/detail、批量规则、分组配置与bundle仍归69。source清单519/520的DeleteRuleGroup/DeleteGroupBundle是共享文件来源，未宣称在68实现。[surface-inventory.md](surface-inventory.md)共享审计/限流仍由81/82负责。

只读Go映射：server/api/rule.go的middleware、GetRuleByGroup、GetRulesByRegion、GetRulesByKey、GetRuleByGroupAndID、SetRule、syncReplicateConfigWithDefaultRule和DeleteRuleByGroup；pkg/schedule/placement/rule_manager.go的AdjustRule、SetRule、DeleteRule；pkg/schedule/checker/checker_controller.go的suspect范围TTL与处理间隔。

[HTTP入口](../../../pkg/server/http_rule.rs)挂入[现有router](../../../pkg/server/http.rs)，[PdServices](../../../pkg/server/lib.rs)经RoleServices与现有[调度队列](../../../pkg/runtime/schedule_scatter.rs)串行访问[宿主适配](../../../pkg/runtime/placement_http.rs)。复用同一个HeartbeatEffectsState.rules与MetaStore；没有新建规则领域实现或后台runtime。RuleManager新增prepare_delete，与已有prepare_rule/publish_rule共同分离校验、fenced持久事务和无IO发布。prepare结果暴露trim后的changed，避免同值写入；持久删除复用StorageEndpoint.delete_rule。调整领域key编码错误以保留start/end、table/txn及codec原因。

[ClusterHost](../../../pkg/runtime/cluster_host.rs)持有canonical SuspectRanges与TTLString，成功修改后加入新旧范围，删除加入旧范围；[原调度任务](../../../pkg/runtime/schedule_worker.rs)每100ms调用check_once，沿用1024范围扫描与同一pending-processed队列。TTL为3分钟、GC为1分钟，宿主取消并join GC；不添加私有runtime或新协调器。

bootstrap和placement开关检查先于JSON解析。空group与跨规则边界region返回null。hex字符串大小写原样保留；URL的+不按query空格处理，百分号解码一次。已验证的JSON语法和标量字段类型错误保留Go的input结构和Offset；适配同时设置tidb-error-code头，当前差分比较状态码、Content-Type与JSON内容。默认pd/default先同步replication配置，后续规则校验失败也保留Go已发生的同步副作用。

## 当前行为证据

[独立HTTP测试](../../../pkg/server/http_rule_test.rs)与[真实存储故障测试](../../../pkg/server/http_rule_fault_test.rs)在lib.rs显式注册。初始测试可编译运行，config/rule落入gRPC fallback，Content-Type实际application/grpc，断言失败退出101，1失败、0忽略。不是用编译失败替代行为失败。

开发轮次保留了编译错误、Go差分发现的空group/null、非法region ID与编码错误文本差异，以及重启时把运行时create_timestamp当成原样持久字段的错误断言。提交前复查另发现encoded slash在匹配路由后可能拆开group/id，现按Go decoded-path匹配语义直接404，并用实际DELETE确认原规则未被误删；该边界也加入Go差分。此前候选全量测试虽然命令退出0，但源码冻结检测到这项后续修复而拒绝验收，未计作最终通过；已重新固定全部输入回归。修正后的重启断言比较规则业务值，并另外用Go和Rust各自重启验证持久化字段/版本行为。Go规则保存调用方版本/时间，在内存发布时再调整版本/时间，不能要求刚发布的元数据在重启后原样保留。

[只读Go监督入口](../../../scripts/http-rule-oracle.py)复用任务004的完整Go/模块哈希守卫、二进制校验、进程组清理与Rust/Cargo输入冻结。对已有bin/pd-server运行，不重建Go服务；binary build info缺少VCS revision，不宣称来自当前HEAD。成功/缺失/禁用/未bootstrap、raw/table/txn key、大小写hex、group/id/+转义、key边界、region不存在/非法ID/跨规则边界、非法count/role/range/hex、JSON语法/字段类型、默认规则同步、拒绝删除最后规则、重复删除均逐项对比。仅归一化create_timestamp，version、状态码、错误结构、JSON值和实际持久副作用仍比较。

同库重启验证规则及删除状态；Go真实重启使用同一durable data目录和网络身份，Rust重启复用同一etcd MetaStore。外部etcd fixture不表示Go embedded-etcd拓扑等价。Rust另外验证follower读取/增删经leader生效，修改范围最终进入原pending队列。真实etcd事务附加非法lease导致拒绝，内存和持久规则都不发布；挂起真实规则事务后宿主取消，HTTP500、持久值不变、worker退出。这些Rust故障注入不冒充Go failpoint差分。

## 最终验证

所有Cargo阶段通过原子mkdir动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，`CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=4 GOMAXPROCS=2`。监督入口另设`PD_COMPAT_GO_BIN=$PWD/bin/pd-server`及`PYTHONDONTWRITEBYTECODE=1`。Rust修改后执行cargo fmt --all。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| 初始cargo test --locked -p astersql-pd-server --lib task_068_ -- --nocapture | 101 | 1失败、0忽略 |
| 最终cargo test --locked -p astersql-pd-server --lib task_068_ -- --nocapture | 0 | 3通过、0失败、1默认忽略 |
| python3 scripts/http-rule-oracle.py | 0 | 1通过、0失败、0忽略；55项HTTP观察及双方重启前后快照 |
| cargo metadata --format-version 1 --no-deps | 0 | 静态检查 |
| cargo fmt --all -- --check | 0 | 静态检查 |
| cargo check --locked -p astersql-pd-server | 0 | 静态检查 |
| cargo check --workspace --locked | 0 | 静态检查 |
| cargo test --locked -p astersql-pd-server -- --nocapture | 0 | 230通过、0失败、29忽略 |

默认忽略的task_068_differential由监督入口显式运行，不把ignored算成通过。其55项HTTP观察与两组重启快照全部相等。create_timestamp只作跨进程归一化；同库重启前后的调用方持久版本变化另行逐项比较。包级测试的零测试doc-test只作编译检查，不计行为数。

状态：已完成。

`cargo test --workspace --locked`：退出0，2130通过、0失败、66忽略。最终1297个Rust/Cargo及监督输入在聚焦、差分、静态、包级、workspace全轮验证后逐项哈希一致。初始及最终1101个Go/模块文件（含ignored/untracked）的路径、SHA-256及Git状态完全一致；plan.md与HEAD原字节一致。Go二进制SHA-256为`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，最终重新核对匹配。

没有执行Go源码写入、Go测试、make、failpoint启停、生成或模块整理。真实服务、etcd fixture及oracle进程组全部结束；自己的槽位锁已释放，编译缓存保留。Python AST、证据文档相对链接、git diff --check通过。原有任务58删除与prompt.md修改不纳入提交。

[压缩原始证据](task-068-evidence.json.gz)，SHA-256 `58ccb8fbe996e96d507bf55e3e6657d362d5a3f2ffddda1835efc75c61f8a0a9`。包括行为失败、开发期编译/差分失败、候选输入冻结拒绝、最终全部命令输出、各阶段实际槽位/绝对target、Go build info和完整只读守卫。

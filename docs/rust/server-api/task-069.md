# 任务069：Placement批量与分组HTTP

状态：已完成。基准 `9de607e84c6b8ea52ed3437aee8ec4a8e48c34d2`；依赖[任务068](task-068.md)与任务001–004已核验。默认features；kvproto固定 `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。Go/模块文件及plan.md只读。

## 来源与装配

[source-inventory.md](source-inventory.md):107、259、519/520及1733–1781的共享placement扫描来源、2601；[wire-inventory.md](wire-inventory.md):167–169、172、177–185，覆盖HTTP-035/036/037/040/045–053全部13条任务69路由。group/key/region规则读取已由68验收。前三项清单及[surface-inventory.md](surface-inventory.md)公共准入已核查；审计/限流等81/82共享能力不在本任务额外宣称通过。

只读Go映射：server/api/rule.go的GetAllRules、SetAllRules、BatchRules、CheckRegionPlacementRule、Get/Set/DeleteGroupConfig、GetAllGroupConfigs、Get/SetPlacementRules、Get/Set/DeletePlacementRuleByGroup；pkg/schedule/placement/rule_manager.go和fit.go的patch/组顺序及拟合JSON。

[批量路由](../../../pkg/server/http_rule_group.rs)复用[单规则入口](../../../pkg/server/http_rule.rs)的门控、错误头和请求取消，经[原router](../../../pkg/server/http.rs)、PdServices和原有调度命令队列到[宿主适配](../../../pkg/runtime/placement_http_group.rs)。同一HeartbeatEffectsState.rules、同一MetaStore、同一leader term；没有第二份领域规则实现、私有runtime或后台协调器。

[领域batch](../../../pkg/schedule/placement/rule_batch.rs)提取无IO的prepare_set_rules/prepare_batch/prepare_set_all_group_bundles与选择器删除；旧领域入口复用这些准备函数并保留Go的120项分段行为。[RuleManager](../../../pkg/schedule/placement/rule_manager.rs)新增prepare_patch，复用RuleList覆盖验证、trim和canonical patch.commit。PreparedRulePatch在宿主提供的TermStore/StorageEndpoint上用**一个fenced事务**保存全部规则与组；成功后同步publish_patch，失败不发布。组配置改变复用suspect ranges和原pending队列。

后端单事务容量是明确边界：本次真实etcd默认上限拒绝150条新增，HTTP500，内存与磁盘均无overflow组规则。HTTP不退回分段提交；这比Go注释允许的分段磁盘部分写入提供更强的失败原子性，不宣称该存储故障行为与Go分段实现等价。普通成功、校验错误与状态变更仍严格差分。SetAllRules按Go先同步pd/default的replication配置；后续规则校验失败时这项既有配置副作用保留，亦已差分，不将其混同为批量patch的部分发布。

region/detail复用fit_region_without_cache；peer到JSON转换留在适配层。[拟合域](../../../pkg/schedule/placement/fit.rs)仅保留orphan列表是否曾分配的元信息，以区分Go的null和清空后的[]。空RuleFit peers及不同角色peers保持null，分数保持Go数字形式。JSON解析校验整批字段类型和字节偏移后才变更；保留RuleOp嵌入Rule字段路径、大小写、重复字段和null顺序。partial/regexp按query键存在性判断，false值仍表示存在；URL加号与bundle删除的第二次PathUnescape保持Go行为。正则复用现有HTTP Go解析器，不复制表达式引擎。

## 行为与差分

[独立测试](../../../pkg/server/http_rule_group_test.rs)在lib.rs显式注册，全部task_069_前缀。初始真实TCP测试能编译运行，缺路由导致Content-Type实际application/grpc，断言失败；cargo退出101，1失败、0忽略。开发期编译失败不计行为失败证据。保存了差分发现并修正的空列表/分数、RuleOp.Rule.count错误字段路径，以及单peer新增场景的一次缺need_buckets编译错误。

最终聚焦3通过、0失败、1默认忽略；忽略的是必须由[只读监督入口](../../../scripts/http-rule-group-oracle.py)显式运行的Go差分，最终另1通过、0忽略。**166项HTTP观察及双方同库重启快照全部一致**，比较HTTP状态、Content-Type和JSON内容/实际读回规则、组、bundle、replication副作用。仅归一化create_timestamp；不移除version或错误字段。包含cold/disabled门控优先级、空/缺失组、组排序与override、batch add/del/prefix/unknown、前缀仅选择原始规则、SetRules upsert、全量/partial bundle、URI组ID与规则组ID冲突、删除最后覆盖规则、非法count/JSON类型/嵌套字段、重复字段/null、大小写字段、正则键存在性与非法正则、URL加号/二次转义、region/detail缺失/非法ID及单peer空列表。

Rust另外验证follower读写落到同一leader状态、修改范围最终进入原pending队列、显式组配置真实持久化及同MetaStore重启。真实etcd事务注入非法lease，分别拒绝两规则batch和规则+组bundle事务，所有内存/持久快照保持原值；挂起真实事务后取消宿主，HTTP500、两条持久规则不变，shutdown join结束。注入仅控制实际后端事务边界，不用mock次数替代业务结果；这些故障/取消场景不冒充Go failpoint差分。

Go oracle复用已有bin/pd-server，不重建Go；build info缺VCS revision，不能宣称其来自当前HEAD。二进制SHA-256 `b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，完整build info在原始证据。Rust为本次源码；外部etcd fixture不表示与Go embedded-etcd拓扑等价。

## 最终验证

原子mkdir从1–10动态领取实际槽位 **1**；全部Cargo、监督入口及子进程使用 `CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。环境 `CARGO_BUILD_JOBS=2 GOMAXPROCS=2`；聚焦/差分/邻接检查使用RUST_TEST_THREADS=4，最终server及workspace包级回归使用RUST_TEST_THREADS=1；监督入口另设PD_COMPAT_GO_BIN为上述绝对Go二进制，PYTHONDONTWRITEBYTECODE=1。代码修改后先cargo fmt --all。

| 确切命令 | 退出码 | 有效测试数量 |
| --- | ---: | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_069_ -- --nocapture` | 101 | 1失败、0忽略 |
| `cargo fmt --all` | 0 | 静态检查 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_069_ -- --nocapture` | 0 | 3通过、0失败、1忽略 |
| `python3 scripts/http-rule-group-oracle.py` | 0 | 1通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-schedule-placement -- --nocapture` | 0 | 44通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 233通过、0失败、30忽略 |
| `cargo test --workspace --locked` | 0 | 2133通过、0失败、67忽略 |

最终1300项Rust/Cargo/监督输入在以上全轮验证前后逐项SHA-256一致。此前候选全轮在补单peer空列表证据前主动取消；该候选整包命令未完成，不计作最终通过。并行包级测试随后发现同一临时端口重绑的AddrInUse竞争（231通过、1失败）；新端口尝试因宿主IdentityConflict被正确拒绝；最终用try_clone保留同一listener FD跨越shutdown，杜绝重绑竞争并保留不可变网络身份及同MetaStore。最终冻结源码后重跑静态/聚焦/差分/邻接。并行整包中的本任务3项全部通过，但任务61/68原有重启测试因AddrInUse失败（230通过、2失败）；保留它们源码，以RUST_TEST_THREADS=1串行重跑整包及workspace。最终各命令均基于同一冻结源码，未重复已通过的无关检查。零测试doc-test只计编译，不算行为测试；ignored不算通过。

开始/结束全部1101个Go/模块文件（含ignored/untracked）的存在性、内容SHA-256和Git状态完全一致。plan.md与HEAD原字节一致。没有Go源码/模块写入，没有Go测试、make、failpoint启停、生成或tidy。Python AST、Markdown相对链接和git diff --check另外核对。原有任务58删除和prompt.md修改不纳入提交。

最终服务/fixture/oracle进程全部结束，自己的槽位锁释放，编译缓存保留。[压缩原始证据](task-069-evidence.json.gz)，SHA-256 `f60fbae44a5656c52eb5b265a20548e0ae5ca0a4d1aa91a22867e3da34a14674`；重复快照和日志按SHA-256引用去重，附restore_source且已完整往返校验；包含初始失败、开发轮次、撤回候选、最终各命令输出/退出码/数量、全部Go及源码哈希与Go监督报告。

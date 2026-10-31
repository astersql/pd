# 任务078：v2领域服务准入边界

状态：已完成（v2服务准入边界；主题8/9领域能力不标等价）。

基准`bd1ff0d96ec5ac746da0e4a1eb09bb2d112d6e94`。批次77的真实ResolvedTS服务、包级及workspace证据已读。总计划只读，沿用现有canonical server/runtime，默认features，未修改依赖或kvproto固定commit `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

## 行为与领域交接

[HTTP边界](../../../pkg/server/http_v2_boundary.rs)在既有路由和角色/取消中间件内装配。已就绪能力继续走真实宿主：ready/livez/readyz使用[探针](../../../pkg/server/http_health.rs)及本机诊断能力；affinity读写使用[现有适配器](../../../pkg/server/http_affinity_read.rs)和活动term的canonical Manager/labeler/事务。未配置本机etcd诊断时保留任务43的显式失败，不以外部MetaStore伪装本机成员。

以下30个Go方法/路径组合逐项注册，返回HTTP503及JSON `{"error":"domain_dependency_unavailable","domain":"…","dependency":"theme-8|theme-9","compatible":false}`，并带`x-pd-compatibility: unavailable`。这是真实准入拒绝，不是模拟领域返回值，不声称这些业务等价或主题8/9完成。拒绝不读取请求体、不执行领域操作；取消的独立边界返回`request_cancelled`且仍为503/compatible=false。外层角色中间件保留既有选主、转发和取消规则。

| 领域 / 依赖 | Go注册方法/路径（省略`/pd/api/v2/`） | 未准入原因 |
| --- | --- | --- |
| keyspace / 主题8 | POST/GET `keyspaces`；POST `keyspaces/id`；GET `keyspaces/:name`、`keyspaces/id/:id`；PATCH `keyspaces/:name/config`；PUT `keyspaces/:name/state`（7项） | 只有metadata持久化与constant crate，没有KeyspaceManager、验证/分配/状态转换及宿主装配 |
| GC / 主题8 | GET `gc/safepoint/:keyspaceID`（1项） | 已有GC state/barrier事务原语；缺GCStateManager的时间、屏障及keyspace语义 |
| TSO组 / 主题8 | POST/GET `tso/keyspace-groups`；GET/DELETE/PATCH `…/:id`；PATCH `…/:id/*node`；POST `…/:id/alloc`、`…/:id/split`、`…/:id/merge`；DELETE `…/:id/split`、`…/:id/merge`、`…/:id/keyspaces`（12项） | timestamp Oracle不是分组管理器；缺分裂合并、节点分配及生命周期服务 |
| meta-service组 / 主题8 | GET/PATCH `meta-service-groups`；PATCH `meta-service-groups/:id/status`（3项） | status持久化不是安全分配/组更新管理器 |
| MCS / 主题8 | GET `ms/members/:service`、`ms/primary/:service`；POST `ms/tso/primary/evict`（3项） | 缺发现/watch/primary生命周期；已有resource-manager Rust配置类型不能提供此服务 |
| maintenance / 主题9 | PUT/DELETE `maintenance/:task_type/:task_id`；GET `maintenance`、`maintenance/:task_type`（4项） | 已有原子任务存储API；本轮保持领域交接，未在HTTP中复制全套维护语义与指标/宿主服务 |

已主动核查[存储endpoint](../../../pkg/storage/endpoint/lib.rs)、[runtime宿主](../../../pkg/runtime/node.rs)及主题8公开API，尝试复用现有能力。真实就绪者已接线；其余按本任务“未满足时显式拒绝且不可标等价”验收。原先把主题8全部实现当作本任务关闭前置的阻塞解释已撤销。主题8/9领域开发及独立兼容证据仍有明确缺口，不属于本边界已完成的业务能力。经典GC等RPC的领域实现未在本任务中新增，也不把其生成桩当作Go契约；任务37固定unsupported watch证据沿用。

处理Gin按方法路由与axum按路径优先级的差异：GET `keyspaces/id`仍进入name lookup对应依赖拒绝；PATCH TSO组的`alloc/split/merge/keyspaces`路径仍进入`*node`对应拒绝。未知v2路径（包括v2根）为纯文本404，已知路径不支持的方法为405。此HTTP拒绝边界不声明对全部Gin路径清理/方法行为的领域等价。

## 失败、通过与差分

[独立测试](../../../pkg/server/http_v2_boundary_test.rs)已在lib.rs显式注册。从只读Go handler注册源码独立解析方法与路径，未复制Rust注册表作为测试oracle。先保留最小接口但不接线：测试实际编译并运行，真实TCP/etcd/tonic bootstrap之后`POST keyspaces`返回HTTP200空体，预期503依赖拒绝，退出101、0通过/1失败。根因是未匹配的HTTP请求落入tonic的HTTP200 gRPC错误fallback；不是编译失败或人为破坏已有能力。

通过测试遍历全部30个组合，携带无效JSON，验证拒绝优先于领域解析；真实etcd中GC sentinel 42与所有被观察领域键值不变。同时验证ready成功、真实affinity创建成功且持久键存在、未知路径404、未知方法405及路径优先级。独立真实TCP测试发送仅header、不上传声明的999999字节body，立即取得503及兼容性拒绝header；取消后再请求maintenance返回取消拒绝，listener及任务正常退出。

首轮实现测试误用了不存在的MetaStore.txn，修正为现有transact API后通过；该编译失败不算行为失败证据。首次差分将MCS未知服务的JSON404误认为未注册路由，修正断言为核验实际JSON handler响应；没有改动产品行为去迎合测试。早期验证期间编辑了测试输入，导致旧affinity只读supervisor正确拒绝inputs hash变化；该轮证据作废，已停止其后续流水线并清理进程/锁，最终固定输入后重新执行全部验证。

[新增只读supervisor](../../../scripts/http-v2-boundary-oracle.py)复用任务4的snapshot、二进制校验、隔离目录和进程组清理。已有Go二进制运行同样的30项请求：未bootstrap领域请求返回真实500；MCS未知服务返回真实JSON404；maintenance在独立Go数据目录实际开始、读取再删除，返回200。Rust统一显式503并标记compatible=false，逐项原始Go/Rust结果保留在压缩证据中；这些差异是已记录交接缺口，绝不算领域差分等价。未bootstrap ready/affinity的状态与JSON确实一致。另显式运行既有health、affinity读写supervisor，验证真实已就绪领域服务的Go/Rust契约。

Go oracle二进制SHA-256为`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`，build info为go1.26.8且没有VCS revision；不声称该二进制由当前HEAD构建。30项实际Go状态为23项500、3项领域JSON404、4项maintenance成功200，原始响应不做等价归一化。

## 来源清单覆盖

前三项清单中含owner=78或78共享边界的原始行已冻结在压缩证据：source-inventory共121行、surface-inventory共508行、wire-inventory共111行（含共享上下文行）。此计数不等同于已迁移领域能力数。

- [source-inventory.md](source-inventory.md)：20、116、221、245、270–277、281、284、305、324、438、530–550、792、796、802、810–878、889、2614–2622、2626。
- [surface-inventory.md](surface-inventory.md)：32–84、165、192、1267、1376、1420、1511、1758、3927–3933、3953–3973、3993、3999–4042、4072–4084、4117–4132、4148–4150、4152、4154–4188、4199–4203、4288、4311–4328、4362、4402–4407、4409–4418、4421–4481、4498–4502、5130–5236、5242–5295、5712、5829、6007–6020、6040–6055、6057、6059–6063、6091。
- [wire-inventory.md](wire-inventory.md)：42–58、82–87、89–121、220、275–276、293–294、297–299、311–327、329–341、381–395、399、401。

其中wire 311–327/329–341是本次30项HTTP依赖拒绝，更新为实际边界并修正原始清单拼接group/suffix的双斜杠；source116指向本证据。ready/affinity沿用43/71/72并补当前运行证据。其余GC/keyspace/MCS/encryption/replication配置、指标、worker、failpoint及manifest行为保留主题8/9/10归属；未宣称本次迁移或逐项业务兼容，完整原文用于审查交接而非扩大transport实现范围。

## 最终验证

所有Cargo及supervisor从仓库根执行，默认features；`CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=1`。本次每个验证阶段均从1–10依序原子mkdir领取实际空槽，本轮全部取得槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。持有进程及会话已写入自己的lock记录；每阶段finally清理锁并保留缓存。

| 确切命令 / 阶段 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| `cargo fmt --all`（每次Rust修改后的阶段首命令） | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_078_ -- --nocapture`（未接线预期失败） | 101 | 0通过、1行为失败 |
| 同上（最终聚焦） | 0 | 2通过、0失败、1默认忽略 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-v2-boundary-oracle.py` | 0 | 1通过、0忽略；30项明确不等价、2项可用边界一致 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-affinity-read-oracle.py` | 0 | 1通过、0忽略；19观察及重启快照 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-affinity-write-oracle.py` | 0 | 1通过、0忽略；真实写入/错误与持久状态 |
| `PD_COMPAT_GO_BIN=$PWD/bin/pd-server python3 scripts/http-health-oracle.py` | 0 | 1通过、0忽略；10条HTTP路径 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 259通过、0失败、39默认忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2160通过、0失败、76默认忽略 |

所有Rust修改后先运行`cargo fmt --all`；最终聚焦、四个只读supervisor、包级、workspace均使用同一固定Rust/脚本输入，哈希已在结束时复核一致。既有affinity包级用例包含真实事务故障与取消；此轮不冒充Go failpoint故障差分。

最终Go守卫的1101项文件（含ignored/untracked）路径存在性、SHA-256及Git状态与开始逐项一致；`plan.md`字节与基准HEAD一致。没有Go/模块差异，无make、failpoint或源码生成写入。Python AST、证据Markdown链接、gzip/JSON往返及`git diff --check`通过。

Cargo、supervisor、fixture及槽位产物进程已结束；自己的slot-1锁已释放，缓存保留。各supervisor守卫确认Go和测试输入不变，Go/Rust隔离目录与进程组完成清理。编号任务按要求标记完成后删除，原有`prompt.md`变更不纳入提交。

[压缩原始证据](task-078-evidence.json.gz)，SHA-256 `d6a27465e52e7eebcce642cd1599ef0c5dc373f0c7e017a9ce2af77bd4d665f8`。包含预期行为失败、开发纠错、作废的中途验证、最终确切命令/退出码/有效测试数量、逐路由Go/Rust原始结果、来源清单行、Go文件守卫、输入哈希及实际槽位/清理记录。

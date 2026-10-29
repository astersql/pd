# 任务043：已实现的健康与区域加载探针证据

本记录保留初版HTTP证据，并在下文补齐用户授权的独立etcd成员诊断验收。最终完成边界为显式成员绑定的过渡部署。基准为
`3ba8aefe8bc9a4b7aa58abb6b09b31403cc87215`；依赖任务042已验收。
固定kvproto版本不变，没有修改Go、Go模块、Cargo依赖或只读plan.md。

## 实现与边界

[http_health.rs](../../../pkg/server/http_health.rs)接入统一router：

- `/pd/api/v1/health`、`/pd/health`共用真实成员发现与逐成员HTTP ping；不以本机领导者角色代表整个集群。复用[grpc_members.rs](../../../pkg/server/grpc_members.rs)的注册表解码，保留成员快照revision；gRPC仍在该revision读取leader。每个客户端URL最多等待3秒，先成功即停止尝试；携带`pd-allow-follower-handle: true`。成员离线时返回200且该成员health=false；注册表不可用返回503，损坏数据返回500。无后台探测任务，取消会丢弃正在拨号/等待响应的连接future。
- `/pd/api/v1/ping`、`/pd/ping`正常返回200空body，宿主取消返回503。
- `/pd/api/v2/ready`、`/pd/api/v2/readyz/leader-promotion`始终本机处理，绕过leader转发。未bootstrap按Go契约视为可提升，verbose返回两个false；已bootstrap但区域尚未加载返回500/503，加载完成返回200。verbose按参数存在性处理。
- [RoleServices](../../../pkg/runtime/member_role_worker.rs)读取持久cluster meta与真实follower同步状态；leader使用已恢复的ClusterHost，并通过资源锁与bootstrap缓存发布/退休同步。不会将“本机是leader”当作完整ready条件。
- [PdNode](../../../pkg/runtime/node.rs)在第一次poll监听服务后才发布ready；提前结束/取消不发送ready，服务结束先清除监听状态。请求取消另由既有request_context传递。

当前明文ping不降级HTTPS，也不声称TLS探测已验收；TLS仍属83–85。
livez/readyz根与子路由、字典序汇总、verbose、重复exclude参数和未知检查404已接线。LocalConsensusProbe明确要求接收请求的宿主本机成员能力；默认返回诊断能力未配置错误，真实RoleServices因此返回503而非伪成功。显式排除检查遵循Go契约，但不能绕过未启动的listener或取消门。初版未交付诊断提供方；该缺口已由下文的独立etcd绑定provider及运行证据补齐。

## 来源映射

- source-inventory：任务43条目，`server/api/health.go`、`server/apiv2/handlers/ready.go`及`pkg/basicserver/`共享生命周期来源。当前仅覆盖上述成员健康、ping、Ready/checkLeaderPromotion及监听生命周期边界。
- wire-inventory：HTTP-145/146/171/172/196/213获得本次实现与测试；HTTP-210/211/212/214–217已有路由/协议和缺失能力时的失败验证，但初版不将失败响应称作完整成功；这些来源行的实际状态检查由下文补齐。
- surface-inventory：health/ping审计注册仍归81/主题9，本次没有新增指标、审计或配置。

## 新运行行为证据

[server测试](../../../pkg/server/http_health_test.rs)和[runtime测试](../../../pkg/runtime/http_health_test.rs)均独立注册。

1. `task_043_behavior`：两个真实PdNode共享真实etcd，leader/follower健康均包含两个真实成员；历史路由、ping和未bootstrap的ready；真实Bootstrap RPC后ready反映已加载；事务加入离线成员后health=false，探测前后注册表entries不变；写入损坏成员记录返回500，节点全部shutdown/join。
2. `task_043_health_pending_cancel_and_store_unavailable`：真实TCP探测已发送请求头但对端不响应时取消宿主，HTTP返回503且探测连接EOF；实际停止etcd后再请求health得到503。没有以mock返回值或调用次数替代故障。
3. `task_043_listener_loading_error_and_cancel`：绑定socket但尚未发布监听状态时ready失败；发布后未bootstrap成功；真实事务写入bootstrap metadata但follower未同步时ready=500、promotion=503，verbose字段准确；读取前后持久entries相同；损坏cluster metadata不成功；取消及停止监听后均失败并join。另验证四项本机诊断未配置时readyz=503、livez=503、individual不会受exclude绕过、未知子检查404、全部排除时文本契约，以及排除不能绕过尚未启动的监听门。
4. `task_043_readonly_go_differential`：由[只读脚本](../../../scripts/http-health-oracle.py)显式运行。使用已有Go PD二进制和真实Rust节点比较10条HTTP请求的状态、Content-Type及body，包括显式排除本机诊断后的汇总和未知检查404。health仅归一化member_id/client_urls，并独立断言Rust实际identity；JSON只消除排版差异，ping与promotion/汇总文本逐字比较；v2 ready保留Gin的`application/json; charset=utf-8`，v1继续使用经典renderer的`UTF-8`。未bootstrap场景真实运行；此项不冒充etcd本机探针、TLS、Go故障注入或生产拓扑等价。

Go二进制SHA-256：`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。已有build info没有证明当前源码revision；未构建Go PD或运行Go测试。etcd测试fixture沿用任务004的只读构建入口。

最初失败命令`cargo test --locked -p astersql-pd-server --lib task_043_ -- --nocapture`已编译并运行，断言成员数量实际1、期望2，退出101（0通过/1失败）。不是编译失败或人为破坏代码。

首次包级回归159通过/2失败/5忽略，失败来自两个已失效的旧假设：task038要求follower健康503，task040依赖health调用healthy()以观察上下文。前者改为两个健康成员；后者改用members入口观察相同请求上下文和断连取消，不削弱断言或让health重新依赖leader。

## 验证命令与槽位

各阶段由原子mkdir动态领取槽位1或2，记录任务/会话/持有进程，所有Cargo及oracle子进程继承：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`或
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
槽位1执行已编译的串行工作区回归时，槽位2验证最后的HTTP头及汇总路由补充；各自使用独立etcd/Go临时目录和随机端口，没有共享测试数据。
最终结果及只读守卫见下方补记。原始命令、退出码和输出将保存在压缩证据中；零测试和默认忽略不计入行为成功。

## 已完成检查与回归过程

| 命令 | 退出码 | 有效测试/结果 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-server --lib task_043_ -- --nocapture`（初始） | 101 | 0通过/1行为失败：成员数1而非2 |
| `cargo fmt --all` | 0 | 每次Rust修改后的格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_043_ -- --nocapture`（最终源码） | 0 | 2通过/1默认忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_043_ -- --nocapture`（最终源码） | 0 | 1通过 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-health-oracle.py`（最终源码） | 0 | 1显式差分测试通过，10条真实HTTP观察 |
| `cargo test --locked -p astersql-pd-server -- --nocapture`（最终源码） | 0 | 162通过/5默认忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture`（最终源码） | 0 | 46通过/32默认忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 工作区元数据 |
| `cargo fmt --all -- --check`（最终源码） | 0 | 格式 |
| `cargo check --locked -p astersql-pd-server` | 0 | 初版实现静态检查，最终实现另由workspace检查覆盖 |
| `cargo check --workspace --locked`（最终源码） | 0 | 全工作区静态检查 |

补充差分的未知检查404曾失败（脚本退出1，Rust测试退出101）：Go Content-Type为`text/plain`，Rust带charset。已修正为Go值，最终10条差分通过；没有放宽该断言。

首轮`cargo test --workspace --locked`在既有`task_006_cancel_and_read_failure_release_resources`的释放端口后重绑定处出现`AddrInUse`，已执行部分为588通过/1失败/33忽略，退出101；同测试在包级及随后串行运行均通过。没有修改这个测试或把端口失败当本任务成功。

随后`cargo test --workspace --locked -- --test-threads=1`普通测试2042通过/38忽略，但运行期间本轮新增了诊断接口，rustdoc后读的新源码与旧编译产物不匹配（`probe_listener_ready` trait项缺失），整体退出1。这是本轮验证安排导致的混合版本，不是无关环境故障，也不作为全量通过证据。最终工作区测试在冻结全部Rust/Cargo源码后重新执行，结果见最后补记。默认忽略不计成功；显式Go差分另行运行。


## 最终补记

冻结1212个Rust/Cargo文件后，`cargo test --workspace --locked`退出0：**2044通过、0失败、38默认忽略**（含2项compile-fail doctest）。运行结束逐文件SHA-256仍与冻结清单相同，最终结果覆盖最后的HTTP头及诊断汇总路由补充。此前失败过程不替代这个最终结果。

1101个Go/模块文件（含ignored/untracked）开始/结束的路径集合、原字节SHA-256及Git状态完全一致；plan.md与HEAD逐字节相同。Python语法、文档相对链接、`git diff --check`检查通过。全部测试/oracle子进程已退出；槽位1、2的自有锁均已释放，缓存保留。

[压缩证据](task-043-evidence.json.gz)包含确切命令/退出码/原始日志、Go binary build info及SHA-256、各轮真实HTTP观察、只读守卫和最终源码哈希。SHA-256：`6af4504c9127793e3060ef11017acfece09c82419b92730e5333b15186c2696e`。未配置能力时的503不算成功；本轮最终验收采用下文用户明确授权的独立etcd绑定契约。


## 用户授权的独立etcd成员诊断（最终实现）

用户明确提出“没有就单独起一个”，授权用显式PD到独立etcd成员的绑定定义过渡诊断，不再要求Go embedded-etcd进程/拓扑等价。此授权解除初版的唯一阻塞；Go和plan.md继续只读。官方[embed接口](https://etcd.io/docs/v3.6/dev-guide/golang_embed_pkg/)为Go服务端，[etcd-client](https://docs.rs/etcd-client/0.17.0/etcd_client/)为Rust gRPC客户端，本轮没有发现满足当前约束的Rust嵌入式服务端方案，采用已有精确版本的客户端连接独立服务，不引入Go FFI或私有runtime。

[EtcdMemberDiagnostics](../../../pkg/runtime/etcd_diagnostics.rs)只连接一个显式endpoint，构造时校验非零预期member ID并固定backend cluster ID。每次status、Range和Alarm响应都重新验证cluster/member身份，防止DNS、代理或错误endpoint指向另一个成员。MetaStore只新增后端中立的可选cluster身份查询，EtcdMetaStore提供真实status身份；无法证明同集群的backend拒绝绑定，不以PD ID冒充etcd ID。

启动前校验诊断与元数据存储集群相同；以真实事务持久保存`/pd/{cluster}/rust/diagnostic-members/{pd-member}`的`etcd-cluster:etcd-member`，拒绝静默重绑。CLI先验证配置及backend，再建立PD身份；已取消的启动不会写入绑定。宿主生命周期持有provider，HTTP取消会丢弃正在等待的RPC。

四项检查均来自真实服务：Serializable/Linearizable Range使用对应选项，CORRUPT来自Alarm API，learner来自该成员status。诊断连接不要求leader，使孤立成员的可序列化本地读仍可存活，线性读无法取得多数派时失败。额外的身份校验或RPC不可达/拒绝也失败；不把它们固定为成功。

部署差异：etcd是独立进程，PD生命周期不负责启动或关闭它；显式成员绑定定义诊断对象，两个进程的故障和权限边界也独立。Go检查内部Server方法，本实现通过绑定成员的gRPC检查，因此连接/权限/超时可以产生503。该调整不证明Rust原生后端或Go进程内拓扑等价；HTTP状态、字段、汇总/排除规则和真实可观察状态仍接受本任务验证。

### 配置与复跑

新增CLI参数必须成对提供：`--etcd-member-endpoint=URL --etcd-member-id=DECIMAL_ID`。ID取真实`etcdctl endpoint status --write-out=json`的member_id，不取PD的名称派生ID。旧调用未配置时仍明确503。库宿主通过`PdNodeBuilder::with_etcd_member_diagnostics`注入同一能力。

例如独立启动etcd（命令为部署说明，本轮验证使用自动清理的临时目录和随机端口）：

```sh
etcd --name=pd-store --data-dir=/absolute/path/pd-etcd-data \
  --listen-client-urls=http://127.0.0.1:12379 \
  --advertise-client-urls=http://127.0.0.1:12379 \
  --listen-peer-urls=http://127.0.0.1:12380 \
  --initial-advertise-peer-urls=http://127.0.0.1:12380 \
  --initial-cluster=pd-store=http://127.0.0.1:12380
etcdctl --endpoints=http://127.0.0.1:12379 endpoint status --write-out=json
pd-server --etcd-endpoints=http://127.0.0.1:12379 \
  --etcd-member-endpoint=http://127.0.0.1:12379 --etcd-member-id=DECIMAL_ID \
  --client-urls=http://127.0.0.1:12381 --peer-urls=http://127.0.0.1:12382
```

Docker引擎不可用时，使用官方[etcd 3.5.15发行版](https://github.com/etcd-io/etcd/releases/tag/v3.5.15)。本轮下载`etcd-v3.5.15-darwin-arm64.zip`，与官方SHA256SUMS核对：`86e189e61f021d9d94e16cdad7c2fda65fd2439ecc4895c45429e3bdc1ca9512`。仅提取etcd/etcdctl两个可执行文件到`/tmp/pd-task043-etcd-v3.5.15`，没有写入任何Go源码或模块；没有把第三方二进制提交入库。

领取共享槽位后，可复跑：

```sh
cargo test --locked -p astersql-pd-runtime --lib task_043_ -- --nocapture
PD_ETCD_BIN=/absolute/path/etcd cargo test --locked -p astersql-pd-runtime --lib task_043_independent_etcd_learner -- --ignored --nocapture
PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/etcd-diagnostics-oracle.py
```

### 新增真实验证

[独立测试](../../../pkg/runtime/etcd_diagnostics_test.rs)显式注册，默认运行2项新增provider测试；2项需要显式环境的验证分别执行，不将默认忽略算作通过。

- `task_043_bound_etcd_member`：真实etcd四类检查成功。最小接口阶段已编译运行并在“真实已初始化etcd必须成功”断言失败（退出101、0通过/1失败），之后才接通真实RPC。首次--locked检查因新增现有workspace依赖需更新lock而拒绝，非行为失败；离线更新仅增加runtime的etcd-client依赖行，没有更换版本。
- `task_043_binding_faults_and_real_alarm`：错误成员ID拒绝；错误后端cluster拒绝且目标namespace为空；预先取消的启动不留绑定；真实PD HTTP livez/readyz及子项成功，稳定成员/绑定namespace前后不变。实际修改持久绑定后拒绝重绑且不覆盖冲突值，恢复后继续。实际Activate CORRUPT得到503，Deactivate恢复200；停止etcd后四种检查均失败。
- `task_043_independent_etcd_learner`（显式）：运行两个官方独立etcd进程；真实member_add learner，status确认为learner，Rust HTTP non-learner=503；真实promote后等待该成员实际应用配置，再验证200及readyz=200。停止另一投票成员造成失去多数派，真实status确认无leader；Serializable检查成功、Linearizable检查失败。进行中的真实线性读由宿主取消，1秒内HTTP返回503，未等待2秒RPC超时。服务、节点、etcd子进程全部join并清理测试数据。learner状态oracle是官方Go etcd的status/member API，不将其说成Go PD learner HTTP差分。
- `task_043_readonly_etcd_diagnostics_differential`（显式）：[只读脚本](../../../scripts/etcd-diagnostics-oracle.py)保持已有Go PD进程存活，Rust provider绑定同一真实存储成员，Rust使用独立PD namespace。9条正常livez/readyz请求逐字比较状态与body；真实激活CORRUPT后比较告警及两种Range探针，再解除告警比较恢复。CORRUPT不等于Range一定失败：初始测试错误地要求503，观察实际Go后改为精确对照真实Go响应，两种读可仍返回200；没有将告警检查的503放宽。
- CLI成对参数、零ID拒绝及既有独立启动/退出测试运行；所有旧health/ready、取消及成员管理回归继续保留。

测试设计中曾将整个PD前缀作为无写入快照，被独立调度worker的启动配置写入干扰；改为稳定的成员/绑定namespace，并对注入的绑定冲突单独核对副作用。learner晋升RPC的提交确认早于follower应用，曾观察晋升后仍503，现按真实status变化等待后断言，没有固定睡眠或直接修改状态。

### 过渡诊断验证命令（最终源码）

本轮各阶段原子领取的实际槽位均为1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
所有构建、检查、测试和oracle脚本继承该绝对路径，正常与失败阶段均释放自有锁，缓存保留。

| 命令 | 退出码 | 有效测试/结果 |
| --- | --- | --- |
| `cargo check --offline -p astersql-pd-runtime`（必要的lock更新） | 0 | 已有workspace依赖，lock只增加一行 |
| `cargo test --locked -p astersql-pd-runtime --lib task_043_bound_etcd_member -- --nocapture`（最小接口失败） | 101 | 0通过/1行为失败 |
| `cargo fmt --all` | 0 | 每次修改后运行 |
| `cargo test --locked -p astersql-pd-runtime --lib task_043_ -- --nocapture` | 0 | 3通过/2默认忽略 |
| `PD_ETCD_BIN=/tmp/pd-task043-etcd-v3.5.15/etcd cargo test --locked -p astersql-pd-runtime --lib task_043_independent_etcd_learner -- --ignored --nocapture` | 0 | 1显式运行通过 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/etcd-diagnostics-oracle.py` | 0 | 1显式Go差分通过，13组HTTP响应比较 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-health-oracle.py` | 0 | 1显式Go差分通过，10组既有HTTP比较 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --nocapture` | 0 | 3通过 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 162通过/5默认忽略 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 48通过/34默认忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式 |
| `cargo check --locked -p astersql-pd-server` | 0 | 指定crate静态检查 |
| `cargo check --workspace --locked` | 0 | 全工作区静态检查 |


### 最终完成证据

状态：**已完成（用户授权的显式绑定独立etcd过渡契约）**。

冻结1214个Rust/Cargo文件后执行`cargo test --workspace --locked`，退出0：**2047通过、0失败、40默认忽略**（含2项compile-fail doctest）。所有冻结源码的路径集合及SHA-256结束时一致。默认忽略的新增独立部署、Go差分各自显式运行并通过；另重新运行既有Go健康探针差分。此前基准2044项为初版证据，最终以2047项为准。

Go/模块1101个文件（含ignored/untracked）前后路径、字节哈希及Git状态相同；plan.md与基准逐字节一致。测试、官方etcd与Go oracle进程全部结束；独立etcd测试目录已清理，槽位1自有锁已释放，缓存保留。Python语法、文档链接、`git diff --check`检查通过。仅新增既有workspace的etcd-client直接依赖，Cargo.lock相应增加一行。

[本轮压缩证据](task-043-provider-evidence.json.gz)包含失败/通过确切命令、退出码、原始日志、官方发行版及二进制摘要、只读Go清单、最终源码哈希和环境。SHA-256：`1e9d29a3de87364ed75eae2bfd096b7f5dfb0b140c3e9c70c13c6669bf5b5701`。原[初版证据](task-043-evidence.json.gz)保留，未改写其历史结果。

来源覆盖：HTTP-145/146/171/172/196由原实现及重新执行的健康差分覆盖；HTTP-210–217由绑定成员的实际读、告警、learner检查及汇总/取消验证补齐。未配置时的503继续是明确的能力缺失响应，完整就绪成功要求按部署说明配置真实成员。此完成结论不延伸到主题10原生后端或Go进程内嵌入等价。

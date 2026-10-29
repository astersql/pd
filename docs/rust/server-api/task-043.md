# 任务043：已实现的健康与区域加载探针证据

本记录验收已实现的经典明文HTTP边界，不代表任务043整体完成。基准为
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
livez/readyz根与子路由、字典序汇总、verbose、重复exclude参数和未知检查404已接线。LocalConsensusProbe明确要求接收请求的宿主本机成员能力；默认返回诊断能力未配置错误，真实RoleServices因此返回503而非伪成功。显式排除检查遵循Go契约，但不能绕过未启动的listener或取消门。本机etcd诊断提供方及其成功/故障差分仍未交付；具体剩余阻塞及最小契约决策仅记录在编号任务文件中。

## 来源映射

- source-inventory：任务43条目，`server/api/health.go`、`server/apiv2/handlers/ready.go`及`pkg/basicserver/`共享生命周期来源。当前仅覆盖上述成员健康、ping、Ready/checkLeaderPromotion及监听生命周期边界。
- wire-inventory：HTTP-145/146/171/172/196/213获得本次实现与测试；HTTP-210/211/212/214–217已有路由/协议和缺失能力时的失败验证，但本机存储能力未验收，不把这些失败响应称作完整兼容成功。
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

[压缩证据](task-043-evidence.json.gz)包含确切命令/退出码/原始日志、Go binary build info及SHA-256、各轮真实HTTP观察、只读守卫和最终源码哈希。SHA-256：`6af4504c9127793e3060ef11017acfece09c82419b92730e5333b15186c2696e`。任务整体仍按编号任务文件的原验收条件处理，不将本机诊断未配置时的503算作完整兼容。

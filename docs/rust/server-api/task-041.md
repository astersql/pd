# 任务041：HTTP领导者转发

状态：已完成（当前经典PD明文HTTP领导者转发边界）。

基准 `7d15ba089af9991c66102a7eca06bd21fbca0ba2`。任务040依赖已由其完成证据及当前公开API核验。Go与模块文件只读，plan.md未修改。验证结果见末节。

## 实现和范围

[http_redirect.rs](../../../pkg/server/http_redirect.rs)装配于[统一router](../../../pkg/server/lib.rs)，仅处理`/pd/` HTTP命名空间，gRPC保留原有入口。领导者身份直接读取宿主已有MetaStore中的`/pd/{cluster_id}/leader`并使用固定kvproto类型解码；是否本地服务复用`PdServices::healthy`，未引入另一个角色缓存或平行选主器。

v1保留本地路径前缀和GET config例外、非空PD-Allow-follower-handle、PD-Redirector单跳保护、当前领导者名称回退、100/200/400/800/1500ms等待。v2无等待，无领导者和单跳错误使用JSON字符串；共用代理的传输错误沿用Go的纯文本http.Error。现有请求上下文提供源IP/端口并追加转发链；请求方法、原始转义路径、重复query、body、响应状态及头通过真实TCP验证。

请求body与响应body流式传输，不聚合整个请求。HTTP连接driver由请求future和响应stream直接持有，无生产spawn或后台线程；客户端断开、响应drop、宿主取消都会释放上游连接。gzip使用工作区固定的已有flate2版本、纯Rust后端解压，按Go代理移除Content-Encoding/Content-Length。依赖清单仅新增server到hyper、hyper-util、flate2的边；无新版本、可选crate feature或新的运行时。

当前宿主监听器为明文HTTP；HTTPS/TLS客户端材料与监听器接线仍由83–85负责，本任务不会将HTTPS地址降级为明文。地址拨号失败可尝试后续地址；已发送的写请求不重新播放body。这里不宣称覆盖Go客户端的跨地址body重用、任意第三方3xx跳转链或损坏响应写出后的重试行为。未来需扩展这些传输边界时必须追加同场景证据。

相邻[任务038测试](../../../pkg/server/http_contract_test.rs)原先明确断言follower members为503；本任务将该断言更新为leader响应，health仍保留原503契约。这是本行为带来的必要回归变更。

## 来源映射

- [source-inventory](source-inventory.md)第79行任务41映射；202/2513行apiutil代理及copyHeader；204/2516行serverapi Redirector、localOnlyPaths、waitForLeader；280/2625行v2 Redirector：由本实现和独立测试覆盖当前HTTP领导者转发边界。
- 249/2589行server/api/middleware.go共同owner 38/41/79/80/81：复用任务40请求上下文，未复制审计或限流。
- 203/2515行multiservicesapi的ServiceRedirector及serverapi中的MCS重写共同owner 41/82：独立MCS不在总计划范围，本任务不启用service-redirector协议；额外本地路由/微服务规则由82验收。
- [wire-inventory](wire-inventory.md)HTTP行的38/40/41/81/82公共适配由统一入口提供转发；单个管理API的领域实现仍由各行owner承担，不能将代理到真实Go leader的成功记为Rust管理API已实现。
- [surface-inventory](surface-inventory.md)没有独立41配置或指标行；不增加配置、指标或全局状态。

## 行为证据

[独立测试](../../../pkg/server/http_redirect_test.rs)显式注册于lib.rs。

- `task_041_behavior`：两个真实PdNode使用同一个真实etcd MetaStore。初次在follower响应503、leader响应200的行为断言失败（退出101，0通过/1失败），实现后逐字节相同；所有节点shutdown/join。
- `task_041_stream_errors_and_cancellation`：真实HTTP上游和代理共用真实etcd，PATCH的UTF-8/换行body、转义路径和重复query写入MetaStore后读取断言；418上游错误保留。另检查拨号回退、gzip、单跳保护、本地ping、allow-follower、自循环、不可达目标、v2无领导者。分别在响应头前和响应body首帧后断开真实客户端，上游请求token取消且持久值不变；宿主取消后服务任务有界join。观察上游仅作为传输边界fixture，不冒充PD领域实现。
- `task_041_leader_wait_clock`：生产MemoryStore与暂停的Tokio时钟验证v2立即返回、v1准确等待3秒及等待期间领导者出现。
- `task_041_readonly_go_differential`：默认忽略，必须用[只读oracle脚本](../../../scripts/http-redirect-oracle.py)显式执行。三个真实Go节点组成集群，Rust代理连接实际Go leader，比较Go follower和Rust代理六组响应，涵盖GET query、POST错误、POST成功、v1/v2单跳错误、本地ping。写入前将Go leader优先级重置为0；Rust转发POST后Go成员API从etcd读取17，证明实际副作用。二者共享的是同一个Go目标集群；Rust发现记录由独立真实etcd保存，不把两种元数据布局混称为同一实现。

Go二进制没有VCS revision，不能声称与HEAD相同；二进制SHA-256、build info、原始响应和进程日志保存在压缩证据。Go内部取消token不可直接观察；取消生命周期由Rust真实TCP测试证明，不声称跨语言token差分。

开发中扩展测试曾发现HTTP未知路径落入tonic fallback，已将中间件移至统一入口并限于HTTP前缀。初次包级验证156通过/1失败/3忽略，失败为上述已更新的任务038旧断言。最初Go脚本误判预期的持久优先级变更；修正后双节点oracle有两次未选出leader而超时，均清理了进程，三节点运行通过。

## 验证记录

所有阶段均动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。原子mkdir后记录任务、计划、会话及持有进程PID；所有Cargo和oracle子进程继承该绝对路径。各阶段结束释放自己的锁，缓存保留。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo test --locked -p astersql-pd-server --lib task_041_ -- --nocapture`（初次失败） | 101 | 0通过、1行为断言失败 |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_041_ -- --nocapture`（最终） | 0 | 3通过、1默认忽略 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-redirect-oracle.py` | 0 | 显式差分1通过、0忽略，6组真实Go/Rust响应及写入状态；最终源码/lock输入哈希守卫通过 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 158通过、3默认忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 2039通过、0失败、36默认忽略 |

工作区计数包含2项compile-fail doctest；零测试不计行为验收。现存deprecated等warning未关闭。依赖边更新后还运行了`cargo check -p astersql-pd-server`以刷新锁文件（退出0）；最终锁文件无新版本，仅增加三条server依赖边。

1101个Go和模块文件（含ignored/untracked）开始/结束的存在性、SHA-256、Git状态完全相同，plan.md与HEAD逐字节相同。没有运行Go测试、make、failpoint或Go写入工具。Python语法、文档链接和git diff --check通过；自己的oracle、fixture、worker和槽位锁均清理，缓存保留。

原始命令、退出码、日志、Go只读快照、输入哈希和Go观察见[压缩证据](task-041-evidence.json.gz)。SHA-256：`7c1e35440687f9c811630bea935301584ccad8e9b7fbe9e1ba89d51690a15bc4`。

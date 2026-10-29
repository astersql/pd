# 任务039：地址与URL安全处理

状态：已完成（监听/advertise地址安全及预绑定端口范围）。

基准 `72e0f2c9c`；已核验前置[任务038](task-038.md)及前三份清单。没有修改Go、Go模块、Cargo依赖或只读plan.md。

## 实现与边界

[Endpoint](../../../pkg/runtime/net_address.rs)复用现有server/config URL语法校验，再施加传输端点约束：HTTP(S)、IPv4/括号IPv6、DNS大小写与尾点规范化、十进制u16端口；拒绝userinfo、路径、query、fragment、列表、空白和zone ID。错误不回显输入或凭据。这是监听/advertise端点约束，不改变通用配置URL语法，也不把Go join中宽松的URL比较函数误作传输校验。

`Endpoint::bind`返回真实Tokio TcpListener；端口0由OS分配，所有权直接交给PdNodeBuilder，不存在关闭后取号重绑。`advertised_url`仅替换显式0端口，保留调用方明确的外部主机/端口（支持NAT）；禁止把通配地址发布为可连接地址。无端口的advertise地址保留默认HTTP端口语义，监听地址必须显式给端口。当前没有TLS acceptor，绑定或发布HTTPS客户端地址明确失败，不将明文socket冒充TLS。

[bootstrap_config与initialize_node](../../../pkg/runtime/node.rs)在写身份/成员之前检查地址；直接嵌入入口也不能绕过校验。实际bound端口进入成员持久记录、HTTP members及GetMembers RPC。仍遵循现有Member::bootstrap不可变元数据契约：已持久化的非规范化旧URL或重启时改变的临时端口不自动迁移，可能触发IdentityConflict；本任务没有增加成员地址更新协议。

[CLI](../../../cmd/pd-server/main.rs)先校验并预绑定，再连接MetaStore/创建身份；所有失败路径由所有权释放listener。现有advertise-client/peer参数开始生效，默认分别继承client/peer。修改CLI是关闭任务所指出strip_prefix缺陷所必需的局部范围扩展。这里只提供单客户端监听器，peer URL仍是成员元数据；没有宣称实现Rust embedded-etcd/peer listener或多URL监听。

## 来源清单与复用

- [source-inventory](source-inventory.md)任务39行、netutil/address.go、tempurl的三份生产文件与测试哈希行、server/join/client_url.go共享39/88行均已核对。
- `netutil.ResolveLoopBackAddr`已有[store-config worker](../../../server/cluster/store_config_worker.rs)的resolve_status_address，保持替换IP但保留原端口的行为及三秒边界；当前包级/工作区回归覆盖原任务14真实HTTP worker。没有重复移植。`IsEnableHTTPS`属于该worker的TLS配置观察，TLS接线仍按[任务014](task-014.md)交后续安全任务；本任务只保证未配置TLS时明确拒绝HTTPS listener。
- Go tempurl的进程内端口表、Linux netstat探测以及pd-ut远程分配器是Go测试基础设施。Rust任务使用OS持有的listener取代取号/关闭/重绑，不引入第二个端口表或Go测试分配域；OS绑定失败直接返回。Go oracle只能用已有独立二进制启动，无法传递预绑定fd，其测试启动释放端口的窗口明确不算Rust预留证据。
- join/client_url.go规范化规则用于本任务端点的scheme/DNS/IP/十进制port子集；成员冲突与join预检仍归88，未伪装已完成成员唯一性。Go允许比较时忽略userinfo/path等，本任务的实际监听端点拒绝这些非端点信息。
- [wire-inventory](wire-inventory.md)没有39独占RPC/HTTP实现行；复用已有members/GetMembers检查真实发布地址。[surface-inventory](surface-inventory.md)client/advertise配置字段的完整调整/多地址/TLS仍由47/90等owner负责，不批量标记通过。

## 行为及差分

[独立测试](../../../pkg/runtime/net_address_test.rs)在lib.rs注册：

- task_039_behavior：最初成功编译后失败于“含凭据advertise必须在持久化前拒绝”，退出101，0通过/1失败。修复后同一真实etcd `/pd/` entries前后相同。
- task_039_endpoint_boundaries：DNS/IPv6/前导零端口规范化、错误URL及凭据脱敏。
- task_039_reserved_listener_lifecycle：IPv4和IPv6真实预绑定、占用期间第二次绑定失败、同一MetaStore、HTTP与tonic GetMembers观察实际端口、检查持久值包含URL，取消并shutdown/join后连接失败且可重新绑定。
- task_039_bind_errors_and_drop：端口冲突、无端口、TLS和通配advertise拒绝，显式外部端点保留、drop释放。
- task_039_invalid_embedded_start_releases_reservation：绕过bootstrap直接start时也拒绝凭据/通配/TLS地址；存储内容不变，监听器释放。
- task_039_readonly_go_differential：默认忽略，仅由[只读脚本](../../../scripts/net-address-oracle.py)明确运行。现存Go PD二进制的IPv4/IPv6健康与成员HTTP观察、四种无效URL的非零退出，与同一Rust端点实现比较。没有把源码推导、HTTP读取或解析单测冒充完整进程拓扑差分；Rust的真实RPC/HTTP和持久状态由上述生命周期测试覆盖。

[CLI测试](../../../cmd/pd-server/main_test.rs)使用端口0进入真实run_until，不再先取号后重绑；额外验证两个advertise参数解析后保留。相邻runtime与server request-role测试的空peer元数据改为合法URL以满足新运行时边界。

Go二进制build info没有VCS revision，不能声称来自当前HEAD；二进制SHA-256、原始响应、启动日志、退出码及守卫结果保存在压缩证据。四种错误为缺scheme、未括号IPv6、非数字端口及非法IPv6。只比较这里实测的成功/拒绝行为，不声明所有Go错误文案逐字相同。成员读取前后相同，进程和独占数据目录清理。

补充测试曾在drop后立即重绑检查失败（退出101，4通过/1失败/1忽略）；已改为有两秒期限的OS绑定可用性观察，避免将同步drop当作异步reactor已完成资源释放。该失败保留在原始日志中。

首次完整工作区回归在server包遇到两项request-role旧测试失败：传入空peer URL触发新校验（150通过、2失败、1忽略；退出101）。修正测试数据为合法peer地址，未放宽生产校验；随后单独重跑及工作区重跑见下表。

## 验证记录

所有Cargo阶段从根目录依次原子mkdir动态领取槽位，实际均为1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁记录任务/计划/会话/PID，子Cargo和fixture继承绝对目录。阶段退出清理自己的锁并保留缓存。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-runtime --lib task_039_ -- --nocapture` | 0 | 5通过；1需oracle显式执行 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/net-address-oracle.py` | 0 | 显式差分1通过、0忽略，6个Go场景 |
| `cargo test --locked -p astersql-pd-runtime -- --nocapture` | 0 | 45通过，32默认忽略 |
| `cargo test --locked -p astersql-pd-cmd-pd-server -- --nocapture` | 0 | 2通过 |
| `cargo test --locked -p astersql-pd-server --lib request_role_test -- --nocapture` | 0 | 2通过 |
| `cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server` | 0 | 构建 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server" python3 scripts/pd-compat-readonly.py --case request-role --case health-member` | 0 | 两项差分各1通过、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-runtime` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 2034通过、0失败、34默认忽略 |

独立进程health/member和受影响request-role场景使用任务4的只读supervisor显式运行，均包含真实Go/Rust服务和清理验证。默认忽略项不计成功，零测试doctest不计行为验收。

工作区2034项包括2032项单元/集成测试和2项compile-fail doctest。开始/结束1101个Go及模块文件（含ignored/untracked）的存在性、SHA-256及Git状态完全一致；plan.md与基准逐字节一致。未运行Go测试、make、failpoint、Go格式化或模块整理。自己的测试与服务进程均结束，槽位1锁已释放，缓存保留。Python语法、文档链接及git diff --check通过。

原始命令、退出码、失败与通过日志、Go响应/build info、运行输入哈希及只读清单见[压缩证据](task-039-evidence.json.gz)，SHA-256：`9d726a1edf6eebe50095adf88145b58723b83c4488ea5c437c080e1359dc8ca8`。没有用历史结果或零测试构建代替行为验收。

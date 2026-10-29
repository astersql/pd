# 任务040：请求上下文元数据

状态：已完成（本任务请求元数据与取消作用域边界）。

基准 `6241e9a28`，已核验任务039完成及前三份来源清单。Go及Go模块只读；没有修改plan.md。协议仍为既有固定kvproto生成接口。

## 实现与边界

[request_context.rs](../../../pkg/server/request_context.rs)由[统一router](../../../pkg/server/lib.rs)装配，请求extensions和公开的`request_context::current()`提供同一元数据；HTTP/RPC领域调用及流式响应逐帧轮询均在请求作用域内。任务局部值不会自动进入另行spawn的任务，宿主必须显式携带取消token并拥有/join这些任务。没有新增logger、后台生产者、运行时或认证策略。

HTTP来源取值遵循Go `GetIPPortFromHTTPRequest`：第一个X-Forwarded-For值优先，其次X-Real-IP、最后连接地址；host:port覆盖X-Forwarded-Port，保留Go对非IP文本、非数字端口的宽松诊断语义。全部forwarded-for字段按顺序保留，peer独立记录真实连接地址。HTTP CallerID按X-Caller-ID、component、anonymous回退；服务标签来自已注册的三个HTTP处理器。gRPC使用完整方法路径和真实peer，并记录pd-forwarded-host，HTTP代理字段不覆盖RPC来源。protobuf CallerId与HTTP CallerID不同，不用HTTP头伪造协议身份。

请求ID保留最多128字节的非空可打印ASCII X-Request-ID，否则生成router局部序号及进程/时间前缀。所读Go RequestInfo没有请求ID字段，这是Rust诊断扩展；不把它写成Go差分已证明的行为。它不是认证凭据，不作为授权输入，也不追加公共响应头。

取消guard覆盖处理future到响应body结束/drop，保留gRPC数据和trailer帧；域调用在断连时随所属future释放，公开子token同时取消，宿主取消向下传播。未消费/复制/记录请求body，未改现有错误和领域提交语义。请求完成不意味着撤销已经提交的事务。

唯一范围扩展是[runtime listener](../../../pkg/runtime/node.rs)使用`into_make_service_with_connect_info::<SocketAddr>()`；否则统一router无法取得实际peer。嵌入方也需提供此扩展，缺失时保持None，不从客户端头猜测peer。复用workspace已有http-body-util版本，仅增加server依赖边，无新版本或feature。

## 来源覆盖

- [source-inventory](source-inventory.md)任务40映射行、requestutil/context.go和request_info.go生产/测试哈希行：覆盖请求信息存取、来源及caller元数据；BodyParam/CaptureBody、String审计格式及WithEndTime由共同owner任务81负责，本任务不复制审计系统。
- 同清单serverapi/middleware.go的40/41/82行及191–203的本地路由条目：本任务保留转发链并沿用来源提取，不新建代理。重定向、额外本地路径注册、实际转发增加链条属于41/82，没有因本任务通过而将它们标为已实现。
- [wire-inventory](wire-inventory.md)RPC/HTTP公共适配中的40由统一入口提供；真实证据覆盖health、AllocID、Tso及错误/取消，其他路由各自领域实现仍由原owner验收。
- [surface-inventory](surface-inventory.md)没有独立40配置/指标实现行；没有修改配置、审计开关或指标集合。Go服务标签通过既有审计日志和指标观察，不新增Rust指标。

## 当前行为与只读差分

[独立测试](../../../pkg/server/request_context_test.rs)在lib.rs明确注册：

- `task_040_behavior`：真实TCP HTTP和tonic，连接同一个真实etcd MetaStore。health领域调用观察来源/链/caller/label/ID；AllocID领域测试宿主将实际观察到的ID写入etcd，读取确认；错误cluster ID返回FailedPrecondition且持久值未改变。Tso成功读取同一存储revision，域调用观察流式上下文；断开等待中的RPC和HTTP，确认取消token触发，持久值不变，服务shutdown/join。
- 测试宿主仅用于观察传输边界和真实存储效果，不把它的固定返回ID或revision派生timestamp当作生产ID/TSO领域实现证据；生产领域仍由已有包级/工作区测试验证。
- `task_040_source_boundaries`：缺头、空首项、优先级、IPv6、非数字端口和非法host:port。
- `task_040_readonly_go_differential`默认忽略，必须由[只读脚本](../../../scripts/request-context-oracle.py)显式执行。脚本复用任务4守卫/现存Go二进制与任务38进程清理逻辑，未编译或改写Go。

Go现存二进制7组真实HTTP失败请求从LocalLogBackend读取ServiceLabel/CallerID/IP/Port，Rust用同一headers调用生产提取函数比较；仅将随机连接端口替换为Go实际观察值。GET health成功及Go指标服务标签也检查；请求前后成员状态一致。成员POST路由仅用来观察Go审计字段，未宣称Rust已经实现该管理路由。Go没有公开请求ID/取消token观察面，因此这两个Rust行为由真实传输生命周期测试证明，不宣称完整跨语言取消token差分。

Go二进制build info没有VCS revision，不能声称来自当前HEAD；哈希、build info、原始响应、审计日志、指标、命令和清理记录保存于压缩证据。

初次失败命令编译成功后在`HTTP domain call must receive request context`断言失败，退出101，0通过/1失败。扩展取消测试时曾错误使用MetaStore禁止的watch revision=0，导致流提前报错、取消断言失败（1通过/1失败）；修正测试为合法revision=1后通过，未改存储契约。

## 验证记录

实际动态领取槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。所有Cargo及子进程继承该路径；owner记录计划/任务/会话/PID，阶段结束释放自己的锁，保留缓存。


| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化 |
| `cargo test --locked -p astersql-pd-server --lib task_040_ -- --nocapture` | 0 | 2通过、1默认忽略；初次预期失败见上文 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/request-context-oracle.py` | 0 | 显式差分1通过、0忽略，7组Go案例及health成功；最终增强标签与输入哈希检查后重跑通过 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 155通过、2默认忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 2036通过、0失败、35默认忽略 |

工作区计数包括2项compile-fail doctest，零测试不计行为验收。现有deprecated字段/辅助测试函数warning不影响通过；未关闭检查。Python语法、文档链接和git diff --check通过。

开始/结束1101个Go及模块文件（包括ignored/untracked）的存在性、内容SHA-256及Git状态完全相同。plan.md与基准逐字节一致。未执行Go测试、make、failpoint或Go模块/格式化写入。自己的Cargo、oracle、fixture及服务进程均已结束，槽位1锁已释放，缓存保留。

原始命令、退出码、测试输出、Go只读快照、输入哈希与oracle观察见[压缩证据](task-040-evidence.json.gz)。SHA-256：`59a331b993758da6cb6f19222c81740b6ee96017a6622b5f83ca5c1c37835017`。

# 任务038：HTTP公共参数与安全错误响应

状态：已完成（公共HTTP契约范围）。

基准 `587e16c9f`，任务37的实现、包级/工作区及只读Go差分证据已核验。本任务验收公共适配层；不将后续HTTP领域路由算作完成。Go、模块文件及只读plan.md不变；没有新增依赖或修改Cargo.lock。

## 契约与接线

[http_contract.rs](../../../pkg/server/http_contract.rs) 提供显式调用的JSON对象读取、十进制u64路径参数、query字节转义和统一JSON响应。解析仅在调用方需要时发生，不向所有HTTP请求强加JSON或Content-Type校验；[既有路由](../../../pkg/server/http.rs) 的GET继续忽略未使用的请求体和query。health/members成功和错误响应统一使用Go renderer的`application/json; charset=UTF-8`。后端/角色错误不再直接暴露给客户端，既有503返回固定JSON字符串`service unavailable`。

Go来源为 [apiutil](../../../pkg/utils/apiutil/apiutil.go) 的ReadJSON/ReadJSONRespondError、ParseUint64VarsField、CollectEscapeStringOption及ErrorResp；实际调用方 [member.go](../../../server/api/member.go) 的SetMemberProperty和DeleteMemberByID。Go使用`json.Unmarshal`读取整个body，不依赖Content-Type；空体/错误类型/尾随JSON返回400，null与空map可成功；未知字段由具体handler决定。Rust保留null与空对象区别、重复键最后值、float64数值语义、非法UTF-8/孤立UTF-16代理项替换。u64拒绝符号、空白、进制前缀、下划线和溢出；query只解码一次，保留二进制字节及`+`与`%2B`区别。

错误体采用`code/msg/data`和`TiDB-Error-Code`；输入错误为400/input，body读取错误为500/internal。**输入错误文案和data不声称与Go逐字相同**：保留EOF及顶层类型说明，其他语法错误使用稳定的`invalid JSON input`，data为null；不复制Go解析器Offset、反射类型或堆栈。内部错误固定为`internal server error`，不序列化原始错误。差分明确比较状态、Content-Type、错误分类及成功值，未把诊断字段差异藏在归一化函数中。本任务完成口径是任务要求的Go等价状态和安全统一错误体，不是Go encoding/json全部诊断的逐字移植。

[server/api/middleware.go](../../../server/api/middleware.go) 没有全局JSON校验；其角色转发、bootstrap、audit、ratelimit分别仍归后续owner。没有添加测试专用生产路由，也没有把解析工具当成member写API实现。强类型配置解码和各handler字段要求仍由对应路由owner实施；不能将map工具直接用于有不同Go目标类型的handler。完整健康/成员语义仍由43/44完成，当前follower的503不是Go follower健康检查等价声明。

## 来源清单

- [source-inventory](source-inventory.md) 的任务38行、apiutil.go生产/manifest共享行与400–402附近路径常量/环境候选行已核对。任务38覆盖参数/错误工具；godotenv加载、CorePath/ExtensionsPath注册及其余apiutil网络代理工具由共享owner41/82及既有CLI负责，不扩张为环境加载实现。
- [wire-inventory](wire-inventory.md) 中263个HTTP行引用的38公共适配前置新增本证据；每条路由的领域操作、请求字段与副作用验收仍归各owner。没有批量改写为通过。
- [surface-inventory](surface-inventory.md) 无38独占配置/指标/CLI行；apiutil测试及中间件共享观测项维持原owner。

## 真实验证边界

[独立测试](../../../pkg/server/http_contract_test.rs) 在lib.rs显式注册：

- `task_038_behavior`：真实runtime、同一真实etcd MetaStore、单监听器HTTP/gRPC装配；两条生产GET路由各接受空/null/非法但未使用的body及无效但未使用的query；真实follower错误路径验证503、安全文案和Content-Type，raft前缀entries前后不变，两个宿主正常shutdown/join。
- `task_038_json_and_parameters`：测试专用axum ingress经真实TCP执行公共parser；正常/null/重复键、空体、尾随文档、数组/标量、NaN/浮点溢出及三种Content-Type。断开声明100字节但只发送一字节的连接后，新请求仍成功，取消并join服务；纯body读取错误使用实际错误stream验证500及秘密文本不出现在响应中。该stream不是MetaStore故障替身，parser没有存储事务。
- `task_038_go_json_values`：UTF-16配对/孤立代理项、转义反斜线、非法UTF-8、float64舍入、null保留。
- `task_038_readonly_go_differential`：必须显式执行。既有Go进程通过真实TCP收到45个JSON/Content-Type场景和5个无效ID；新采集结果送入同一Rust公共parser，验证状态、响应类型、错误码及成功值；无效ID错误文案精确比较。Go成员响应前后完全一致。Rust公共parser的TCP路径由上一测试覆盖，不把此单测说成已经存在同名Rust管理路由。

[只读oracle脚本](../../../scripts/http-contract-oracle.py) 复用任务4的binary/snapshot守卫，验证已有Go构建信息，等待leader选出再运行参数场景。只使用已有Go PD二进制，不运行Go测试或生成器；只读fixture构建沿用任务4已审查入口。进程、临时目录及子Cargo在正常/失败/信号路径清理；报告保存原始Go响应、Rust输出、输入哈希和只读守卫结果。Go binary无VCS revision，不声称来自当前HEAD；原始build info及SHA-256见压缩记录。

首轮失败验证成功编译，`task_038_behavior`在真实HTTP的Content-Type断言失败：实际`application/json`，预期`application/json; charset=UTF-8`；退出101，0通过/1失败。随后实现并取得通过证据，没有以编译失败代替行为失败。

## 验证记录

所有Cargo阶段均从仓库根按1–10原子mkdir动态领取；实际槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。锁记录任务、计划、会话、持有PID；构建及子进程继承该绝对目录。Rust修改后先运行`cargo fmt --all`。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 静态 |
| `cargo test --locked -p astersql-pd-server --lib task_038_ -- --nocapture` | 0 | 3通过、1需oracle显式执行 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-contract-oracle.py` | 0 | 显式差分1通过、0忽略；45 JSON + 5 ID场景 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 153通过（152单测+1集成）、1默认忽略 |
| `cargo fmt --all -- --check` | 0 | 静态 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态 |
| `cargo check --workspace --locked` | 0 | 静态 |
| `cargo test --workspace --locked -- --test-threads=1` | 0 | 2028通过、0失败、33默认忽略 |

工作区2028项包含2026项单元/集成测试及2项compile-fail doctest；忽略项不计通过。本任务oracle在最终脚本版本下重新显式运行，1通过、0忽略，报告`http-contract-oracle-1793289466914597000.json`的readonly_unchanged、inputs_unchanged、work_directory_cleaned均true。Go binary SHA-256为`b9baaf7b6160a6ea1e3e3dc72ae059f317b0066838b676680105daceed297bdc`。

开始/结束1101个Go/模块文件（包含ignored/untracked）的路径、SHA-256和Git状态完全一致；plan.md逐字节不变。没有运行make、failpoint启停、Go测试、Go格式化或模块整理。自己的所有Cargo与服务进程已结束，槽位1占用锁已释放，缓存保留。Python AST、文档链接和`git diff --check`通过。

原始失败/通过日志、完整命令与退出码、Go响应及build info、只读清单和执行输入哈希见 [压缩证据](task-038-evidence.json.gz)，SHA-256：`f4bbe60e89c57262b76e102c5fc79e9fbec97db83a4d4c8ead3417a91bf54021`。表中的测试数来自本次实际运行，未用零测试构建或历史日志替代行为证据。

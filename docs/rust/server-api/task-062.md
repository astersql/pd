# 任务062：Operator只读HTTP

基准 `a35127e3a`。已读取只读plan.md、任务62、任务001–004/061证据及三份清单；批次61依赖已满足。Go源码与Go模块全程只读。

## 来源与实现

来源行：source-inventory.md:100、251；wire-inventory.md:133、136–137（HTTP-001/004/005）；surface-inventory.md:4730、4733–4734。source-inventory.md:511–512列出的删除行为属于任务63，此处不声称写接口已覆盖；审计与限流仍交81/82。协议继续固定kvproto `f8ad079c8e9812ed5900b0c12b5f1d97171464c4`。

- [HTTP适配](../../../pkg/server/http_operator_read.rs)提供GET `/pd/api/v1/operators`、`/operators/{region_id}`、`/operators/records`；[router](../../../pkg/server/http.rs)与[PdServices](../../../pkg/server/lib.rs)显式注册。
- [宿主投影](../../../pkg/runtime/schedule_operator_read.rs)经现有[scheduling命令队列](../../../pkg/runtime/schedule_scatter.rs)、[RoleServices](../../../pkg/runtime/member_role_worker.rs)及[Application](../../../pkg/runtime/node.rs)读取canonical Coordinator、Handler与ControllerRecords。未创建平行服务或把协议/HTTP类型引入领域层。
- 复用领域Operator.render、OpRecord.render与to_json_object。完整文本断言发现ordinary/joint builder仍使用占位brief，故在[ordinary builder](../../../pkg/schedule/operator/builder_simple.rs)与[joint builder](../../../pkg/schedule/operator/builder_joint.rs)修复必要的说明文本；保持Go分支优先级及promote/remove分支的既有倒序文本，不更改调度步骤或领域状态机。
- [独立TCP测试](../../../pkg/server/http_operator_read_test.rs)、[宿主时钟测试](../../../pkg/runtime/schedule_operator_read_test.rs)、[brief测试](../../../pkg/schedule/operator/http_brief_test.rs)分别显式注册。运行时只增加chrono-tz 0.10.4及已有iana-time-zone 0.1.65的集中workspace依赖，以保留宿主IANA时区、偏移与缩写；Cargo.lock固定传递依赖，无协议版本或可选feature变化。

Go只读源码映射：[HTTP](../../../server/api/operator.go)、[Handler](../../../pkg/schedule/handler/handler.go)、[Controller/status records](../../../pkg/schedule/operator/operator_controller.go)、[Operator文本/object/record](../../../pkg/schedule/operator/operator.go)、[状态映射](../../../pkg/schedule/operator/status.go)、[时间解析](../../../pkg/utils/apiutil/apiutil.go)。

## 行为与证据

`object`按参数存在性选择object输出，`object=false`仍返回object；kind按Go拼接并保留重复值，未知kind复用此前匹配集合，全部未知时跳过bootstrap/领域读取。无筛选空列表为[]，有kind的空字符串列表为null，object空集合为[]。records空匹配返回500与ErrOperatorNotFound；ID与from解析错误为JSON字符串400。保留query解码、首个from、空from、正负int64边界及数字扫描中首个溢出/坏字符的优先级。

Go的time.Unix把Unix秒转换为year-1秒时采用有符号wrapping；因此`from=9223372036854775807`仍包含当前记录。真实Go差分推翻了“极大正数一定排除全部”的初始测试假设，现保留这个可观察边界，并验证溢出阈值前后相邻两秒。记录筛选为包含截止秒，TTL继续由canonical ControllerRecords负责。

宿主为一个term保存单调/墙钟映射，GET不重置已有时间；保留Go零时间、纳秒小数去尾零、时区偏移/缩写及m=单调文本。记录finishAt与duration在重复读取间保持不变。受控时钟验证UTC、Shanghai/CST、负方向推进与123456700ns的七位小数；未把Rust时钟测试冒充Go时间推进差分。

真实TCP+tonic心跳建立三个region及store，ScatterRegion产生两个Operator，通过实际transfer-leader指令检查HTTP对象、运行状态、完整文本模板及重复kind，回报新leader后验证SUCCESS与历史记录；重新admit后以真实conf_ver递增触发CANCEL，按实际region ID验证取消状态与保留记录。独立测试覆盖follower默认转发、显式本地follower错误，以及真实MetaStore事务被阻塞时排队的HTTP读取随宿主关闭返回500并完成shutdown。读取前后真实etcd raft元数据逐项相同。重启保留元数据，但与Go一致清空内存Operator历史；不会把历史查询伪装成持久Operator日志。

[只读监督入口](../../../scripts/http-operator-read-oracle.py)复用任务004二进制校验、Go内容守卫、进程组清理及精确测试计数。双方使用现有Go bin/pd-server与真实Rust宿主；Go二进制build info没有VCS revision，不声称由当前HEAD构建。完整参数/方法/空值/状态结果及重启结果作相等比较；Scatter的随机分配以每个实际region ID、peer目标、epoch和RPC指令逐项校验，时间字段仅在完整文本模板断言中替换为<time>，并单独验证格式与稳定性。没有删除ID、计数、状态、错误或步骤字段以制造相等；原始objects/strings/records保存于输出。默认ignored差分由监督入口显式运行，其通过数不计入默认回归。

初始红灯命令可编译并运行1个真实TCP行为测试，退出101，缺路由返回application/grpc而非application/json。开发中另修正专用bootstrap隔离、Go heartbeat流首响应等待、brief占位文本、i64时间溢出、重启复用成员地址；这些开发失败不是成功证据。Go只读约束优先于旧make/failpoint说明，没有运行会改写Go的入口。

## 最终验证

最终阶段动态领取槽位1；`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。默认features，CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=4、GOMAXPROCS=2。以下命令均退出0（初始红灯除外）。

| 确切命令 | 退出码 | 有效测试数 |
| --- | ---: | --- |
| 初始 `cargo test --locked -p astersql-pd-server --lib task_062_ -- --nocapture` | 101 | 0通过、1行为失败 |
| `cargo fmt --all` | 0 | 静态检查 |
| `PD_COMPAT_GO_BIN="$PWD/bin/pd-server" python3 scripts/http-operator-read-oracle.py` | 0 | 1通过、0失败、0忽略；含RUNNING/SUCCESS/CANCEL与重启差分 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 静态检查 |
| `cargo fmt --all -- --check` | 0 | 静态检查 |
| `cargo test --locked -p astersql-pd-server --lib task_062_ -- --nocapture` | 0 | 2通过、0失败、1忽略 |
| `cargo test --locked -p astersql-pd-runtime --lib task_062_ -- --nocapture` | 0 | 1通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-schedule-operator --lib task_062_ -- --nocapture` | 0 | 2通过、0失败、0忽略 |
| `cargo test --locked -p astersql-pd-server -- --nocapture` | 0 | 215通过、0失败、23忽略 |
| `cargo check --locked -p astersql-pd-server` | 0 | 静态检查 |
| `cargo check --workspace --locked` | 0 | 静态检查 |
| `cargo test --workspace --locked` | 0 | 2115通过、0失败、60忽略 |

状态：已完成。最终1278个Rust/Cargo输入在格式化之后冻结，全部验证命令结束后逐文件SHA-256一致。开始/结束1101个Go/模块文件（含ignored/untracked）存在性、字节SHA-256及Git状态一致；plan.md原字节与HEAD一致。所有测试、服务、fixture及worker已结束，自己的槽位1锁已释放，缓存保留。Python AST、本文相对链接与git diff --check另作提交前核验。原有任务58删除与prompt.md修改不纳入本任务提交。

[压缩原始证据](task-062-evidence.json.gz)，SHA-256 `d0b51610559fdabd4a187a028ff8409997c68bb0d02403d3fe6a3845f1705427`。包含红灯/开发失败、最终命令与输出、实际测试计数、二进制构建信息、Go/Rust原始HTTP状态、前后Go及最终Rust/Cargo哈希清单、槽位与清理记录。预回归期间补充了CANCEL/witness用例，其输入冻结检查未通过，单独保存且不作为最终证据；随后完整重跑并通过最终冻结检查。

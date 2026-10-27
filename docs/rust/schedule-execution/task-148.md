# 任务148：范围散射配置传输适配

依赖147提交 `8d6f2bf5d`、[task-147](task-147.md) 与最终完整验证证据已核实。
复用 canonical ScatterRangeScheduler/config/update/persist/reload、真实调度引擎和server依赖；
未新增crate、feature或Cargo依赖，未改只读plan与原主题任务。

## 契约来源

用户后续明确授权：允许只读Go源码/测试推导加Rust真实行为和故障验证替代缺失的
Go运行oracle/运行差分。本任务未执行Go，不声称取得Go运行输出。
[provenance](logs/task_148/provenance.json) 记录来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、四份Go源码/测试、oracle-index、
依赖证据、后端错误定义与[fixture](fixtures/task_148.json) SHA256。
四份Go文件与来源commit逐字节一致；fixture标记 `runtime_go_oracle=false`。
oracle-index无本任务HTTP专属运行oracle，未修改其原始证据含义。

只读 `TestScatterRangeBalance` 覆盖placement rules开/关下范围内leader/region均衡、
空end-key以及特殊tolerant ratio，不覆盖HTTP方法、JSON、更新或保存失败；
本任务不将测试名冒充运行证据。传输期望来自scatter_range.go的专属handler、
apiutil.go的ReadJSONRespondError/ErrorResp及既有JSON错误格式，不使用BaseScheduler响应。

## 实现与测试驱动

[入口](../../../pkg/server/schedule_admin_scatter_range.rs) 为
`serve(&mut ScatterRangeScheduler, &StorageEndpoint, &ClusterPaths, Request<Body>) -> Response`，
[边界说明](../../../pkg/server/schedule_admin_scatter_range.rs.md) 记录领域与wire分工。
更新使用现有领域API；列表按字段声明顺序和Go转义规则编码，成功body为 `null`；
range-name不可变。保存失败保留新内存配置和旧持久化快照，响应为500、错误字符串再追加null，
精确保留Go在错误分支后没有return的行为。非法类型字段不修改原值；非法JSON返回400/input，
请求流错误返回500/internal且不应用部分输入。没有完整server/api路由装配。

[红测](logs/task_148/red.json) 实际运行同名行为测试，错误候选返回501，
在空对象更新期望200处断言失败，exit101（1失败）；修正后同测试通过。
后续新增调度测试曾因引用未声明的keyutil不能编译，已通过删除不必要的keyrange输入解决，
未添加依赖；故障期望文本先误写为通用字符串，已对照MetaStore错误定义修正。
所有尝试日志保留，不以编译错误作为所需行为红测。

[测试](../../../pkg/server/schedule_admin_scatter_range_test.rs) 在server/lib.rs显式注册：

- `task_148_behavior_cases`：31个固定HTTP案例，含方法/路径、空对象/null、错误类型、
  未知/大小写字段、重命名、逆序/空key、百分号/NUL/Unicode、重复字段，以及语法/顶层类型错误。
  校验status/body/Content-Type、错误header、无Allow、内存与持久化配置。
- `task_148_persistence_and_body_faults`：真实MemoryStore边界保存失败，校验500精确拼接body、
  新内存/旧持久化、reload与重试；2个部分/空输入流故障，校验internal响应且状态不变；
  3条无需读body的路径用一旦poll即panic的流检查；额外检查列表字段顺序、缩进和转义字节。
- `task_148_schedule_after_http_update`：3种leader/region限流场景 × placement开/关 ×
  conf-change v1/v2 × 更新前/后，共24次真实决策。
  初始x–z范围不命中；POST更新至a–b后leader优先，禁leader时生成region操作，OpRange限流时无操作。
  验证desc、kind、region、源/目标、步骤、分配peer、confver和影响量；配置持久化与内存一致。
  使用真实BasicCluster、RuleManager、StatisticsSnapshot、MemoryStore和canonical balancing engines；
  只控制允许的时间/随机/peer分配输入。

仅语言相关错误stack作结构检查，不比较Go/Rust栈文本。存储故障文本来自真实Rust MetaStore错误，
Go契约证明它透传保存错误，未声称该注入文本来自Go运行。没有忽略错误或调度差异。

## 验证环境与结果

[脚本](logs/task_148/verify.py) 原子mkdir动态领取仓库共享槽位1–10，记录任务/会话/PID，
设置绝对CARGO_TARGET_DIR；异常/中断等待子进程结束后核对owner并释放，保留缓存。
默认features，CARGO_BUILD_JOBS=4，RUST_TEST_THREADS=2。
[Go拦截器](logs/task_148/reuse-go/go) 拒绝实际Go执行，仅复用SHA256
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`
的既有etcd二进制；协议ZIP只读复用且校验固定SHA256。
[复用记录](logs/task_148/go-reuse.jsonl) 保存请求/路径/摘要，没有Go构建、文件改写或failpoint注入。

## 最终结果：已完成

[完整命令与原始日志](logs/task_148/final.json)、
[有效测试汇总](logs/task_148/final-summary.json)：

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 检查通过 |
| `cargo test -p astersql-pd-server --locked --lib task_148 -- --nocapture` | 0 | 3通过、0失败、0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 95通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查通过 |
| `cargo fmt --all -- --check` | 0 | 检查通过 |
| `cargo check --workspace --locked` | 0 | 检查通过 |
| `cargo test --workspace --locked` | 0 | 1895通过、0失败、1忽略 |

聚焦3项测试覆盖31个HTTP案例、24次真实调度决策及保存/请求体故障；
workspace原有1项忽略未改变，本任务没有新增忽略或跳过必需检查。
各阶段实际动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`；
所有子进程结束后owner与锁已释放，缓存保留。

[最终审计](logs/task_148/audit.json) 对比[开始快照](logs/task_148/baseline.json)：
1101份Go/模块文件集合、内容SHA256及Git状态不变；两级只读plan不变；
Markdown链接和日志哈希有效，`git diff --check`退出0。Cargo清单和锁文件无变化；
共享入口仅server/lib.rs本任务模块/测试注册。已有其它任务文件删除排除在本次提交外。
验收后删除148编号任务，不改来源主题或plan。

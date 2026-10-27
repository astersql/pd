# 任务150：领导者扰动配置传输适配

状态：已完成。前置149提交 `72d14c987`、[task-149](task-149.md)及其完整验证日志
已经核实。复用 canonical ShuffleLeaderScheduler；没有新crate或feature。

## 来源和契约

用户明确授权允许只读Go源码/测试推导加Rust真实行为和故障验证，替代缺失Go运行
oracle/运行差分。本任务没有运行Go或声称取得Go运行证据。
[provenance](logs/task_150/provenance.json)记录来源commit、四份Go来源摘要、fixture及
oracle-index摘要；四份来源均与 `836fdc7cf3ef34f7eabc9d051418995bb97efabe` 逐字节一致。
oracle-index仍是source_assertion，未伪填runtime_fixture。

`shuffle_leader.go`嵌入BaseScheduler且没有ServeHTTP覆盖；`base_scheduler.go`
默认方法忽略请求，返回200、`text/plain; charset=utf-8`、精确字节`not implements`。
这不是JSON响应，也没有专属配置更新、参数错误或保存失败HTTP分支。
Go相邻 `TestShuffleLeader` 只覆盖空集群、leader/admin操作及transfer-in限额，
不覆盖HTTP或持久化失败；仅只读检查了源码，没有把测试名称当成运行证据。

[HTTP fixture](fixtures/task_150.json)冻结19条方法/路径/输入/状态/响应场景，
显式标记runtime_go_oracle=false。调度输入和期望复用[task101](fixtures/task_101.json)
的24条源码推导场景，不从本次实现生成。响应字节、header、错误、状态、操作不归一化。
HEAD在直接handler层校验body，宿主负责实际wire层抑制；完整路由装配属于主题7。
调度仅沿用task101明确随机/时钟边界：store排序后首/末索引、单region抽样、
固定1_000_000_000_000ns，分别检查heartbeat v1/v2。

## 生产边界与测试驱动

[生产入口](../../../pkg/server/schedule_admin_shuffle_leader.rs)：
`serve(&ShuffleLeaderScheduler, Request<Body>) -> Response`，说明见
[同名文档](../../../pkg/server/schedule_admin_shuffle_leader.rs.md)。模块/独立测试显式注册。
方法只实现继承的HTTP契约，不复制调度器，不新增动态配置接口或访问storage。
Cargo清单和锁仅增加已有workspace storelimit测试依赖，远程依赖版本未变化。

[红测](logs/task_150/red.json)以错误候选501运行相同真实行为断言：退出101，
0通过/2失败，200状态断言明确失败。随后恢复Go契约200。
[首次绿测](logs/task_150/green.json)记录新增测试依赖要求更新锁的101错误；
[锁更新与通过](logs/task_150/lock.json)记录离线锁更新和4个测试通过。
离线更新带来的无关pulldown-cmark版本变化已排除，最终验证使用原版本。

[测试](../../../pkg/server/schedule_admin_shuffle_leader_test.rs)包含：

- `task_150_behavior_cases`：19条HTTP×3种配置状态=57条，涵盖所有常用方法及自定义
  方法、未知路径/查询/尾斜杠、空/无效/错误类型/有效JSON；精确比对status、body、
  content-type、无Allow与错误header，非JSON响应，内存配置保持原样。
- `task_150_unread_body_faults`：4条一旦poll即panic的外部流，证明不读取失败或阻塞
  请求体，不改配置。
- `task_150_persistence_faults`：真实MemoryStore仅I/O边界注入Unavailable，实际保存
  失败后4条HTTP请求仍200；恢复后持久化字节和重建配置保持原样，另验证5条严格限额边界。
- `task_150_schedule_after_unsupported_update`：24场景×请求前/后×heartbeat v1/v2，
  共96次真实决策。HTTP尝试把成功场景改成范围外、把无操作场景改成全范围，配置与
  持久化均不变，结果仍符合冻结期望。真实core索引、过滤器、placement、builder验证
  步骤、源/目标、kind、Low、confver、influence、heartbeat、应用后完成状态。
  包含无region/store、pending/down、无合法目标、pause/reject/learner/witness、
  范围命中/越界、disconnected/限流/offline/special-use、placement拒绝/允许和不重试。

## 验证及审计

[verify.py](logs/task_150/verify.py)原子mkdir从1–10动态领取槽位，记录任务/计划/会话/PID，
所有阶段实际使用槽位 **1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
默认features，CARGO_BUILD_JOBS=4、RUST_TEST_THREADS=2。
正常、失败、中断均等待子进程结束后释放自有锁，保留缓存。
[Go拦截器](logs/task_150/reuse-go/go)拒绝真实Go命令，只复用哈希核验的已有etcd
二进制和固定kvproto ZIP；[复用日志](logs/task_150/go-reuse.jsonl)保存实际请求和摘要。
没有Go生成/构建/测试、make或failpoint写入。

[最终命令/退出码/原始日志](logs/task_150/final.json)及
[有效测试汇总](logs/task_150/final-summary.json)：

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务相关格式变更 |
| `cargo test -p astersql-pd-server --locked --lib task_150 -- --nocapture` | 0 | 4通过、0失败、0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 102通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1902通过、0失败、1既有忽略 |

只出现既有cpu_usage弃用warning。本任务未添加忽略或跳过必需检查。
[审计](logs/task_150/audit.json)对照[开始快照](logs/task_150/baseline.json)，检查
全部Go/模块文件（含untracked）和两份只读plan的集合/内容哈希及Go Git状态不变；
日志哈希与本地Markdown链接有效，git diff --check退出0。
仅提交本任务代码、测试、依赖接线、说明及证据；其它任务的既有删除全部排除。
验收后删除150编号任务文件，不修改plan或原始主题。

# 任务152：见证领导者配置传输适配

状态：已完成。前置151提交 `85a11132b` 的全部七项必需命令退出0，日志哈希、
[证据](task-151.md)及当前API已核实；没有增加crate、feature或更改Cargo依赖。

## 来源与边界

按用户明确授权，采用只读Go源码/测试推导和Rust真实行为/故障验证替代缺失Go运行
oracle。没有运行Go，也没有声称Go运行差分。既有oracle-index保持原有证据分类，
未将source_assertion改写成runtime_fixture。
[provenance](logs/task_152/provenance.json)记录source commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、源码/fixture/index哈希和前置提交；
四份Go来源均与该commit字节相同。

只读 `transfer_witness_leader.go` 嵌入BaseScheduler且没有ServeHTTP、EncodeConfig或
ReloadConfig覆盖。BaseScheduler返回200、`text/plain; charset=utf-8` 和精确字节
`not implements`；无JSON、无换行、无参数解析、无配置更新、无存储访问。
继承EncodeConfig编码nil为`null`。因此不创造不存在的专属配置/持久化500接口。
完整server/api路由属于主题7；HEAD检查位于直接handler层，线上的body抑制由宿主负责。

`TestTransferWitnessLeader`覆盖限额耗尽/恢复和多目标转移及完成；
`TestTransferWitnessLeaderWithUnhealthyPeer`覆盖pending/down和队列耗尽；最后同时
pending/down分支没有重新入队，不能当作该输入的运行证据。两者不覆盖HTTP或存储
故障。未裸跑需要failpoint的Go测试，也不把列出名字当运行证据。

[HTTP fixture](fixtures/task_152.json)固定22条源码推导输入/响应；
[调度fixture](fixtures/task_103.json)复用14条既有源码推导输入/期望。
HTTP状态/header/body逐字节检查，无归一化；禁止将纯文本解释为JSON。
调度固定时钟10^12ns、peer/store ID 1–3，候选按ID稳定排序，以首/末随机索引覆盖
两个目标；目标数组顺序、kind、优先级、错误和命令字段均精确比较，不丢弃行为差异。

## 实现与测试驱动

生产路径 [schedule_admin_transfer_witness_leader.rs](../../../pkg/server/schedule_admin_transfer_witness_leader.rs)
提供 `serve(&TransferWitnessLeaderScheduler, Request<Body>) -> Response`，
独立测试在现有server/lib.rs显式注册，复用canonical scheduler，不添加配置类型。
同名说明文档记录领域边界、来源和错误/故障语义。

初次测试编译因StorageEndpoint没有Clone失败；修正为Arc共享后，
[行为红测](logs/task_152/red.json)使用错误501候选，退出101，0通过/4失败，
明确由相同测试的501与200断言检出错误；初次编译错误仅为
[辅助诊断](logs/task_152/compile.json)，不是行为失败证据。
修正200后新增逐批队列检查并执行最终验证。

[测试](../../../pkg/server/schedule_admin_transfer_witness_leader_test.rs)有效行为：

- `task_152_behavior_cases`：22条方法/路径/请求×空/1/4事件队列=66次响应检查，
  含自定义方法、HEAD、未知路径、query、空/非法JSON、null、数组、伪配置参数。
  所有请求后实际调度队列，验证事件数、三条批次上限及最终耗尽。
- `task_152_unread_body_faults`：POST/GET/DELETE/HEAD四条poll即panic的外部流，
  证明失败/阻塞body不被读取，响应保持精确200纯文本。
- `task_152_schedule_after_unsupported_update`：14场景×请求前/后=28次真实调度，
  每个成功操作检查两种conf-change心跳命令。覆盖健康/普通领导者、首/末目标、
  pending/down、learner、missing/offline/disconnected、耗尽预算、witness/demoting
  目标及无leader错误；检查多目标/步骤、kind、Urgent、influence、confver、错误类型、
  命令和最终完成，之后队列为空。请求在事件入队后发送，证明不能丢弃既有事件。
- `task_152_persistence_faults`：真实MemoryStore后端，仅I/O边界注入Unavailable；
  实际读写均失败时四种HTTP请求仍200，已排队事件实际产生Urgent多目标操作，
  恢复I/O后持久化仍为原始`null`，无配置更新副作用。

## 验证与只读审计

[verify.py](logs/task_152/verify.py)每阶段原子mkdir动态领取1–10槽位并记录计划、
任务、会话和PID；所有实际阶段使用 **槽位1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
默认features，CARGO_BUILD_JOBS=4、RUST_TEST_THREADS=2。正常/失败/中断路径均等待
子进程退出才释放自己的锁；缓存保留。最终自有锁已释放。
[只读Go拦截器](logs/task_152/reuse-go/go)拒绝实际Go命令，仅复用哈希校验的已有
etcd测试二进制和kvproto ZIP；[实际复用记录](logs/task_152/go-reuse.jsonl)。
没有Go构建/测试、make、failpoint、生成或模块写入。

[最终命令/退出码/日志](logs/task_152/verified.json)及
[测试汇总](logs/task_152/verified-summary.json)：

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务Rust文件格式变化 |
| `cargo test -p astersql-pd-server --locked --lib task_152 -- --nocapture` | 0 | 4通过、0失败、0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 110通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1910通过、0失败、1既有忽略 |

仅出现既有cpu_usage弃用warning；无新增忽略。
[审计脚本](logs/task_152/audit.py)及[结果](logs/task_152/audit.json)核对
[开始快照](logs/task_152/baseline.json)，验证全部Go/模块文件集合、内容哈希和Git状态
（含untracked）及两份只读plan哈希不变，Markdown链接及日志哈希有效，
`git diff --check`退出0。Cargo清单和锁没有修改。

只提交本任务适配、测试、说明和证据，其它任务已有删除排除；验收后删除152编号
任务文件，不修改两份plan或来源主题。

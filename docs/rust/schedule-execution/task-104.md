# 任务104：范围均衡作业配置状态

状态：已完成。

依赖103由提交 `fcf87b951028f4b53f6ef81384797278de979f5a`、
[task-103](task-103.md)、canonical SchedulerConfig / StorageEndpoint / MetaStore API
和其158项包测试、1707项workspace测试记录核实。没有把编号任务删除当作依赖证据。
本任务串行执行，无子代理；整体、包级和来源主题 plan 保持只读。

采用用户后续明确授权：以只读 Go 源码/测试推导及 Rust 真实行为和故障验证，替代
缺失 Go 运行 oracle/差分准入。[provenance](logs/task_104/provenance.json) 固定当前
来源提交、原计划基准和来源/fixture SHA-256，runtime_oracle=null。已读取
oracle-index.json，未套用其中与本作业配置无关的运行结果；没有执行 Go 或 failpoints。

## 实现和源码契约

[balance_range_job.rs](../../../pkg/schedule/schedulers/balance_range_job.rs) 提供
`JobStatus`、`JobTime`、`BalanceRangeJob::{from_args,from_http,is_complete,expired,should_finish}`
及 `BalanceRangeJobs::{new,from_args,jobs,peek,add,delete,begin,finish,gc,encode_config,reload}`。
[模块说明](../../../pkg/schedule/schedulers/balance_range_job.rs.md) 记录领域/宿主边界。
lib显式登记生产和测试模块。复用真实 SchedulerConfig、StorageEndpoint、MemoryStore，
不新增存储实现。chrono复用workspace已固定0.4.42；Cargo.lock仅增加本crate到chrono
的依赖边，无任何远程版本变化。serde_json启用raw_value以保留Go精确token匹配。

只读 Go `balance_range.go` 固定30分钟默认超时、七天保留期、状态精确token编解码、
未完成alias重复拒绝、add ID分配、取消补齐时间、begin/finish前置状态、严格大于过期、
队首连续GC和保存失败恢复旧快照。`init.go`固定slice工厂ID0、空alias允许、参数解码；
`utils.go`和`keyutil/keyrange.go`固定HTTP两次QueryUnescape、二进制键、hex持久化。
Go core.Rule零值是LeaderScatter，不能套用Rust core.Rule默认Unknown。

实际相关Go测试为 TestCodecConfig、TestJobExpired、TestJobGC、TestPersistFail、
TestAddBalanceRangeJobWithInvalidFieldType。TestPersistFail需要failpoints，本任务未运行。
任务列出的 TestPlacementRule/TestIsBalanced/TestPrepareBalanceRange/TestTIKVEngine/
TestLocationLabel 是范围求解/放置相关测试，不能充当本队列配置故障的运行证据。

## 案例和红灯

[fixture](fixtures/task_104.json) 含28条源码推导HTTP输入/预期：六种rule/engine组合、
默认/空/小数timeout、两次解码、非UTF-8/NUL键、多范围、无界范围、错误类型、非法
engine/rule/alias、timeout无单位/零/负/溢出、range数量不符、两阶段无效转义。
适用mode为默认features下classic/nextgen共享配置领域，没有引入业务feature。

归一化：clock使用显式Unix纳秒；RFC3339Nano规范化UTC，忽略时区偏移文本和Go
monotonic元数据，保留绝对时间及纳秒精度。范围逐字节比较；不归一化status、ID、
错误类别、timeout或队列顺序。JSON对象键顺序不参与比较。

4个独立测试：

- task_104_behavior_cases：28条fixture，7条slice参数（6错误+1正常），逐字段校验
  rule/engine/alias/status/timeout/create、范围数量和hex键。
- task_104_status_codec_and_time：5个状态编码/回读，7个未知token保持已有状态且新对象
  pending，包括转义拼写；Rule零值/转义，4个时间编码回读、边界超时/过期、未开始错误、
  省略start/finish、完整job序列化回读。
- task_104_queue_lifecycle：真实保存/恢复，重复alias、错误状态、begin/finish、取消完成
  拒绝、缺ID/索引错误、alias复用、peek、保留既有start、七天等号边界、全清空ID重用、
  工厂ID0。
- task_104_persistence_faults_and_gc_prefix：MetaStore read/transact故障，add/delete/begin/
  finish/gc失败后完整本地快照不变；stall后取消future不发布候选；重建读到最后成功
  快照；GC不跨越未完成队首；无效JSON reload保持原队列。只在真实I/O边界注入故障。

initial记录缺API编译红灯exit101；focused记录core/keyutil KeyRange适配编译错误；
focused2的4测试通过。随后[red_behavior.py](logs/task_104/red_behavior.py)分别故意将
默认timeout增加1ns、将begin结果改为pending；相同fixture/lifecycle断言均失败，
各0通过/1失败、Cargo exit101。脚本在finally恢复生产原始字节；脚本exit0表示两次
红灯成功识别，不能误读为变异实现通过。最终验证在恢复且修正Go Rule零值后运行。

## 验证

所有Cargo阶段通过[run.py](logs/task_104/run.py)原子mkdir动态领取槽位；实际均使用
槽位1，`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划/任务/会话/PID，正常/异常/中断回收子进程后释放自己的锁，缓存保留。
最终阶段 `CARGO_BUILD_JOBS=1`、`RUST_TEST_THREADS=2`。

最终串行命令链全部exit0：[final.json](logs/task_104/final.json)，
[完整stdout/stderr](logs/task_104/final.log.gz)，[计数](logs/task_104/test-counts.json)。

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| cargo fmt --all | 0 | 仅本任务Rust格式差异 |
| cargo test -p astersql-pd-schedule-schedulers --locked --lib task_104 -- --nocapture | 0 | 4通过/0失败/0忽略，28条fixture+7条slice及状态/故障 |
| cargo test -p astersql-pd-schedule-schedulers --locked | 0 | 162通过/0失败/0忽略 |
| cargo metadata --format-version 1 --no-deps | 0 | workspace元数据通过 |
| cargo fmt --all -- --check | 0 | 通过 |
| cargo check --workspace --locked | 0 | 通过，既有cpu_usage弃用warning |
| cargo test --workspace --locked | 0 | 1711通过/0失败/1既有忽略，含2文档测试 |

所有阶段自己的锁已释放，缓存保留。文档链接和脚本语法检查通过。
Go/模块/只读plan共1119文件起始哈希逐一核对，
[audit](logs/task_104/audit.json) 无变化、无新增Go/模块文件、Go Git状态为空，
git diff --check exit0。其它任务既有编号删除保留，不暂存进本提交。

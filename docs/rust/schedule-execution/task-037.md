# 任务037：操作终态归档与成功回调

状态：已完成。

生产路径 [controller_records.rs](../../../pkg/schedule/operator/controller_records.rs)，
领域边界 [controller_records.rs.md](../../../pkg/schedule/operator/controller_records.rs.md)，
测试 [task_037_test.rs](../../../pkg/schedule/operator/task_037_test.rs)。
仅注册 operator crate 的模块/测试，无新增 Cargo 依赖、feature 或锁文件变更。

## 依赖与来源

已核实批次36提交 `a1da40abc` 及 [task-036.md](task-036.md)：真实 admission
identity removal、dispatch removed 成员、Operator 的状态/Record/History API 可用。
本任务重新验证整个 operator crate 和 workspace。已读取 oracle-index.json，
其中 operator_concurrent_replace 的旧运行结果不代表本任务完整故事。

用户授权解除 Go 运行 oracle 准入限制。本任务 fixture 明确标记
`provenance=source_assertion`、`go_execution=false`，**没有运行 Go，没有 Go 运行差分**。
来源 commit `836fdc7cf3ef34f7eabc9d051418995bb97efabe`；
[fixture](fixtures/task_037.json) 冻结 controller、controller test、metrics、cache/ttl
四个 Go 来源文件的 SHA256。[audit.json](logs/task_037/audit.json) 核实当前字节与来源
commit 一致，并保存 fixture 哈希。

只读采用的契约：

- SetSuccessCallbacks 替换回调列表、跳过 nil；成功派发调用成功 hook。
- buryOperator 按原始终态产生 finish/replace/expire/timeout/cancel 指标；非终态
  为 unexpected，并尝试 Unknown 取消。仅成功记录 running duration 和 finished counters。
- records 按 region ID 覆盖，保留10分钟；状态查找优先 running map。
- GetRecords/GetHistory 按归档 FinishTime 做包含等号的过滤；History 内时间来自
  查询时刻，失败操作也生成历史。清理不影响活动操作。
- cache TTL Get 在 expire.Before(now) 时失效；GetAllID 要求 expire.After(now)，
  因此精确到期时 direct lookup 与 list 的边界不同。
- TestPollDispatchRegionForMergeRegion 的两次失败路径要求 source/target 同时移除，
  两个归档记录均存在。TestConcurrentAddOperatorAndSetStoreLimit 既有 Rust 对应
  场景随全包回归，不将 Go 测试名字列举冒充运行证据。

## 实现与边界

`ControllerRecords::{bury_operator,bury_removed,record_dispatch,get_status,get_records,
get_history,clean_all_op_records,set_success_callbacks}` 使用 canonical Arc 操作。
宿主消费 admission 移除/替换成员和拒绝批次、poll 移除成员与 dispatch outcomes。
`record_dispatch` 只消费身份比较成功后返回的 removed，不独立消费 completed。
没有复制 operator map、cluster、limiters 或 transport；宿主装配按前序任务边界继续。

任务要求的“一次归档和通知”以对象身份弱引用 guard 实现，超过 TTL/清空后仍被
外部持有的操作不能再次通知；同 region 的新对象允许替换归档。已释放对象 guard
在下一次 burial 回收，不延长 Operator 生命周期。回调在归档发布并解锁后执行，
允许查询和重入。回调 panic 传播，但不污染记录锁，也不会导致重复通知；不承诺
panic 后执行剩余 hook，也不承诺跨进程 exactly-once。

`TerminalObservation` 是宿主 exporter 的指标输入；事件与成功持续时间仅产生一次，
finished counters 可作为成功 callback 注册。本层不安装全局 Prometheus registry。
nonwitness TTL、fast influence、等待队列晋升复用前序派发/宿主边界。
本行为不访问持久化，不需要 MetaStore backend 或 I/O stub。

## 测试与故障

5个真实测试，fixture 表驱动6个状态案例全部通过（6成功/0失败）：CREATED 的
unexpected 取消、SUCCESS、CANCELED、REPLACED、EXPIRED、TIMEOUT。
另覆盖成对 merge 失败、重复事件、region 覆盖、活动状态优先、TTL 精确边界与超过
TTL、包含等号的 from、查询时刻 History、clean 不清活动操作/不解除 guard。
真实 ControllerDispatch 成功移除后归档和通知，第二次派发无重复；OpRecord duration
使用真实状态时间。成功 hook 替换、None 跳过和回调重入；8线程并发 burial 仅一个
观察事件；清理释放强引用；host callback panic 后记录仍可查询且重试不会再次调用。

只控制显式单调时钟和宿主 callback 故障。fixture 期望来自 Go 只读源码推导，
不是待测 Rust 输出；没有归一化状态、错误、事件、持续时间或历史目标。

[red.log](logs/task_037/red.log) 保存缺失 module/API 的编译失败，exit101。
[mutation.py](logs/task_037/mutation.py) 在最终同一5个测试上把 SUCCESS 事件
故意改为 cancel，实际3通过/2失败、exit101，见
[mutation.log](logs/task_037/mutation.log)。finally 恢复源码，然后重新格式化、
聚焦和全量回归通过；由此证明测试能识别行为错误，未仅依赖编译错误。
捕获的 `host callback fault` panic 是预期故障测试输出，不是失败/忽略用例。

## 验证与资源

[run.py](logs/task_037/run.py) 每阶段按1–10依次原子 mkdir 领取共享 Cargo 槽位，
记录 plan/task/session/PID，所有子进程继承绝对 CARGO_TARGET_DIR；finally 在进程组
退出后释放自己的锁，保留缓存。所有阶段实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
JSON 保存确切命令及退出码，log 合并 stdout/stderr。

最终完整验证及数量见下表；workspace 使用前序任务已验证的两线程环境避免
既有 etcd 测试并发环境问题，没有跳过任何测试。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_037 -- --nocapture` | 0 | 5通过、0失败、0忽略；fixture 6组通过 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 90通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1453通过、0失败、1个既有忽略用例 |
| `git diff --check` / `git diff --cached --check` | 0 | 通过 |

完整命令链见 [validation.json](logs/task_037/validation.json)，输出见
[validation.log](logs/task_037/validation.log)，有效数量见
[summary.json](logs/task_037/summary.json)。日志仅移除末尾多余换行，JSON 保存
原始 SHA256 与移除数量，可重建原始输出。workspace 的1个既有 ignored 测试为
`tso_processes`（需要独立 Go/Rust process binaries）的已有声明；本任务未新增 ignored、过滤或跳过行为。有效通过数包含
2个既有 compile-fail doctest。全部槽位已释放，缓存保留。

最终 Go/模块文件1101个，前后 manifest 哈希相同，Git 状态无 Go 差异，含 untracked。
fixture 哈希与来源字节一致。两份只读 plan.md 不变。原有其它编号任务删除不纳入
本任务提交；提交只包含本任务生产模块、注册、测试、fixture 和证据。

# 任务048：规则边界拆分检查

状态：已完成。

生产路径为 `pkg/schedule/checker/split_checker.rs`，公开
`SplitChecker::{default, get_type, check}`，复用 PauseController、真实 core、
placement、持久化 labeler 和现有 create_split_region_operator。只增加已有
workspace hbstream 领域依赖以指定 USEKEY；Cargo.lock 仅增加此路径依赖边。

## 依赖与来源

批次47由提交 `69e47ea84` 和[任务47证据](task-047.md)确认，包含真实目标选择、
拆分冷却和成功的workspace基线。本次checker包级回归再次覆盖前序21个测试。
已读取整体/包级只读plan.md、oracle-index.json、ADR与移植指南。

用户后续授权解除Go运行oracle/运行差分准入限制。本任务使用**只读Go源码与测试
契约推导 + 实际Rust行为验证**；没有运行Go、没有Go输出、没有声称运行差分。
[provenance](logs/task_048/source-provenance.json)记录来源commit、Go文件SHA256、
fixture SHA256，Go输出哈希为null。TestSplit实际源码有placement aa/cc和标签
bb/dd优先两个场景；其余边界由Check、GetSplitKeys、range list及operator factory
源码契约推导。时间直接传入Unix秒、键按原始字节比较，无归一化。

## 行为与测试

[冻结fixture](fixtures/task_048.json)含23个固定案例：Go两例、无边界、禁用placement、
禁用placement仍允许labeler、相同边界标签优先、各来源内部去重、严格排除region
端点、内部键、外部键、开放起止、全范围无切点、二进制字节排序、标签在外时回退、
暂停/精确到期/恢复、joint-state错误、joint无边界不调用factory、无leader仍可拆分。
期望为手工固定Go源码契约，未由待测Rust实现生成。

Go明确先查询labeler，仅在没有labeler边界时查询启用的placement。因此两个来源
**不做跨来源合并**；测试专门验证标签优先，边界排序和去重交给真实canonical index。
labels从真实MemoryStore/StorageEndpoint加载并持久化规则，placement从真实
RuleManager初始化/写入规则，未以mock替代组件。

成功案例逐项检查description、region ID/epoch、SPLIT kind、默认priority、单步
start/end/keys/USEKEY、confver、未完成状态、v1/v2命令键与policy、每store region
和leader influence。Joint-state返回BuilderError::OriginJoint，不产生部分operator；
Go在此处日志后返回nil，Rust保留类型化诊断供宿主处理。本check无存储写入或ID分配，
无需伪造无关I/O故障。宿主负责统计、日志和后续执行；本单元不新增metrics或runtime。

## 失败与错误识别

[red-formatted.log](logs/task_048/red-formatted.log)记录已注册测试对空实现的真实
行为失败：go-placement应生成operation却得到None，exit101，0通过/1失败。
red.log为同一断言在格式化前的首次运行，不用编译失败代替行为证据。
最小实现后同一测试通过。

[mutation.py](logs/task_048/mutation.py)依次注入忽略暂停、忽略配置、破坏标签优先级、
丢失一个placement键四类错误；[结果](logs/task_048/mutation-results.json)均exit101，
由行为断言识别，非编译失败。finally恢复正确源码，最终验证在恢复后重新执行。

## 验证与审计

[run.py](logs/task_048/run.py)每阶段原子动态领取共享槽位，保存任务/会话/PID，
等待子进程结束再释放锁，异常/中断终止并等待子进程，缓存保留。所有阶段实际领取
槽位1：`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
完整stdout/stderr和各命令exit code在logs/task_048；workspace使用
`RUST_TEST_THREADS=2`。有效数量见commands.json。

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_048 -- --nocapture` | 0 | 1测试，23案例通过 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 0 | 22通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1480通过，0失败，1既有忽略 |

[before.json](logs/task_048/before.json)及[audit.json](logs/task_048/audit.json)核对所有
Go/模块文件（含untracked）和两个只读plan.md的哈希、Go/模块Git状态，确保未变；
未运行Go/make/failpoint。`git diff --check`通过。全workspace格式化审查只修改本任务
新Rust文件；没有更改其它生产文件。既有其它编号任务删除排除在提交外。

workspace唯一既有忽略为tso_processes；没有新增忽略。槽位锁已释放，缓存保留。
日志仅清理行末空白与多余末尾空行，不改命令、断言或退出码。

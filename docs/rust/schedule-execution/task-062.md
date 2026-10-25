# 任务062：拆分键分组与区域校验

前置批次61已由提交 `d50c8d226fa814f0872ebb3197529bc54fd40788` 和
[task-061](task-061.md) 核实，包含聚焦、包级与workspace通过证据。当前批次62
仅处理本任务，未启动并行代理。生产路径为
[split_keys.rs](../../../pkg/schedule/splitter/split_keys.rs)，公开
`group_keys_by_region` 与 `RegionGroupKeys`，边界见
[说明](../../../pkg/schedule/splitter/split_keys.rs.md)。

遵循用户后续授权：**只读Go源码/测试契约推导 + Rust行为验证**。
未运行Go、未生成Go oracle，不宣称运行差分。[provenance](logs/task_062/provenance.json)
保存来源commit、四个Go来源及fixture SHA256；Go来源均与计划基准
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 相同，Go binary/output hash明确为null。
已核查[oracle-index](oracle-index.json)，其历史运行案例不覆盖本任务，未改写索引。

来源契约与输入输出：

- `region_splitter.go` 的 `groupKeysByRegion/checkRegionValid`：按当前真实区域
  查找，依次检查复制、leader、start-key。复制不足按每个key调用
  `addSuspectRegions(false, id)`；重复或边界key也会通知，不能提前去重。
- Go实际保留重复内部key和输入顺序；任务目的中的“去重”仅按现有行为解释为
  排除已存在的region start，组按region ID唯一。未增加全局排序或去重策略。
- `filter/healthy.go` 与 `placement/fit.go`：legacy要求精确副本数且无learner；
  placement要求count/role/orphans满足。pending/down本身不是分组拒绝条件。
- `TestMain`仅环境入口；`TestGroupKeysByRegion`检查bbb/ddd归组、fff起点跳过、
  zzz不存在；`TestRegionSplitter`的fff/ggg与aaa/bbb涵盖正常/不存在区域，
  其中mock handler不能作为真实split派发或新region扫描证据。本次Go测试未执行。

[fixture](fixtures/task_062.json)固定17个原始输入和期望groups/suspect：空输入、
Go分组案例、边界/空洞、重复/乱序、无限边界、少/多副本、learner、无leader、
复制先于leader和边界检查、legacy pending/down、rules满足/少副本/pending/down、
missing与不健康混合。测试逐字段比较组ID、组内keys和suspect调用顺序/false标记，
确认Arc身份。另一个测试对真实RegionsInfo执行split替换后重新查询，证明已拆key
被跳过且另一个key归新region，旧快照不污染新查询。共2个测试、18个场景。

测试采用真实core索引、replication filter、RuleManager及真实MemoryStore初始化。
无mock分组、mock健康判定、生产故障开关或外部I/O注入。错误/异常覆盖表现为准确
跳过及suspect输出；该行为无持久化或外部I/O，因此不宣称存储故障覆盖。
组迭代以升序ID固定Go未指定的map顺序；组内顺序及错误/状态未归一化或忽略。

TDD记录：

- [red.log](logs/task_062/red.log)：编译成功，空结果临时实现导致同组2个测试
  均发生行为断言失败，退出101；不是缺包/缺API错误。临时实现已替换。
- [fixture-correction.log](logs/task_062/fixture-correction.log)：初版错误期望
  rules pending拒绝；依据Go `RuleFit.IsSatisfied/RegionFit.IsSatisfied`复核后
  修正pending/down两条fixture为允许，未从待测实现生成期望。保留失败记录。
- [focused.log](logs/task_062/focused.log)：同组2个测试通过，17条fixture全部执行。
- [package.log](logs/task_062/package.log)：包级2通过、0失败，零doctest不计有效测试。

Cargo由[run.py](logs/task_062/run.py)逐阶段通过原子mkdir动态领取1–10共享槽位，
写owner包含计划、任务、会话和PID。异常/中断先终止并等待子进程组，再释放锁，
保留缓存；所有命令继承绝对CARGO_TARGET_DIR。每次Rust修改后先cargo fmt --all。
各命令、实际退出码及target记录在同名JSON，stdout/stderr保留在log。
`cargo generate-lockfile --offline`后仅保留新增路径crate，恢复附带远程依赖漂移；
所有后续检查使用--locked，远程依赖条目未变化。

最终验证全部退出0：

| 命令 | 结果 |
| --- | --- |
| `cargo test -p astersql-pd-schedule-splitter --locked --lib task_062 -- --nocapture` | 2通过，18场景 |
| `cargo test -p astersql-pd-schedule-splitter --locked` | 2通过、0失败 |
| `cargo metadata --format-version 1 --no-deps` | workspace解析通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过 |
| `CARGO_BUILD_JOBS=1 cargo test --workspace --locked` | 1538通过、0失败、1既有忽略 |
| `git diff --check` | 通过 |

[完整workspace日志](logs/task_062/workspace-test.log)与[计数](logs/task_062/counts.json)
包含2项compile-fail doctest；既有tso_processes默认忽略不计成功。
既有server/core_region_adapter.rs:134 cpu_usage deprecated警告保留，无新增警告。
所有阶段实际使用**槽位1**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
最终子进程已全部退出、自己的槽位锁已释放，缓存保留。

[审计](logs/task_062/audit.json)：1101个Go/模块文件的内容哈希、文件清单及Git状态
保持不变；两个只读plan.md未改；远程Cargo.lock版本/校验和保持不变。
workspace格式化无范围外Rust变更。证据Markdown相对链接均可解析。
日志仅清理行尾空格和末尾多余空行，未改变断言、结果或退出码。
既有其它任务删除未纳入提交。任务062验证完成，编号文件按要求删除；
本证据为后继批次的稳定依赖记录。

状态：已完成。

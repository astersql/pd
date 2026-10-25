# 任务017：影响量聚合与成本

状态：已完成。依赖批次16提交 `5fbe374ab`、[task-016](task-016.md)
及其真实tracker API/验证记录已核实。

用户授权解除 Go 运行 oracle/运行差分准入，测试期望依据只读源码及测试契约。
`evidence_kind=source_assertion`、`go_execution=false`；没有执行 Go operator，
没有将源码推导声称为 Go/Rust 运行差分。已审查 oracle-index.json：其并发
controller oracle 不提供此任务的成本阈值和资源属性输出，未借用为本任务证据。

## 实现与边界

生产路径 [influence.rs](../../../pkg/schedule/operator/influence.rs)，领域说明
[influence.rs.md](../../../pkg/schedule/operator/influence.rs.md)。API 为
`StoreInfluence::{add,resource_property,get_step_cost,add_step_cost,adjust_step_cost}`
和 `OpInfluence::{new,get_store_influence,update_store,snapshot,add}`。
复用真实 core 空 region 判定、core constant 资源/策略及 storelimit 类型和成本表。
无 proto、业务 I/O、mock、私有运行时、全局状态或新 feature。

多 store 的有符号影响量逐项累加，成本按四种已知 limit type 合并；未知类型
可直接读写，聚合按 Go 的 TypeNameValue 忽略未知源成本。缺失 store 建立零值，
缺失成本返回0；全部 i64 加法保持 Go wrapping 语义。
`RwLock` 保护整张 map，快照深拷贝成本。Rust 用 owned 读取及锁内更新代替
Go 返回可变指针，防止字段/map 别名引入数据竞争。先读源快照再锁目标，支持
自加与并发互相聚合；不提供跨容器事务一致性。

成本 >20 MiB 为1000，非空小 region 为200，size<=1且keys==0为空。
小 region 非空时 snapshot/leader-in 类型没有成本表项；错误输入按 Go 索引
语义 panic，测试捕获并核实未写入成本。空 region 不访问成本表。

## 固定契约与红绿

[fixture](fixtures/task_017.json)冻结来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、四份只读Go来源SHA256、输入和期望；
[verify.py](logs/task_017/verify.py)逐字节核对当前源码和来源commit。
`TestAdjustStepCostUsesRegionKeys` 的5个案例提供基础键数契约；等于20、空/负
统计、资源未知策略、未知成本及聚合/并发/溢出从相关源码推导，未称为Go运行结果。

[task_017_test.rs](../../../pkg/schedule/operator/task_017_test.rs)显式注册5项测试：

- `task_017_behavior_cases`：9个size/keys组合 × 2 peer类型 =18个成本案例，
  每个检查默认、调整和抵消；9个资源/策略案例；另2种大region与空region成本。
- `task_017_aggregation_and_snapshot_ownership`：缺失store、两个操作累加、
  五种字段/四种成本、未知type排除、快照map隔离、自加、负值抵消和源不变。
- `task_017_concurrent_snapshots`：4线程各250次真实写入与读取，
  每个快照核对三个字段一致，最终1000次更新无丢失。
- `task_017_invalid_small_cost_is_programmer_error`：4种不支持的成本索引，
  均捕获panic且成本仍为0。
- `task_017_wrapping_and_reciprocal_adds`：i64最大值溢出及两线程互相聚合。

初次 `--locked` 因新增本地依赖缺少锁条目而失败（[red-missing.out](logs/task_017/red-missing.out)），
此失败不算行为红灯。离线生成锁后恢复与任务无关的远程依赖版本，最终Cargo.lock
仅增加operator的三个现有workspace路径依赖。
真实红灯把 `>20` 改成 `>=20`，同一行为测试实测 `1000 != 200`，退出101，
见[red.json](logs/task_017/red.json)、[完整输出](logs/task_017/red-threshold.out)。
[red.py](logs/task_017/red.py)提供可复现注错及finally恢复流程；测试不由待测实现生成期望。

## 验证

[validation.json](logs/task_017/validation.json)及对应 `.out` 保留完整stdout/stderr、
确切命令、退出码和有效测试数：

| 命令 | 退出码 | 有效结果 |
| --- | ---: | --- |
| `cargo fmt --all` | 0 | 已格式化 |
| `cargo test -p astersql-pd-schedule-operator --locked --lib task_017 -- --nocapture` | 0 | 5通过、0失败 |
| `cargo test -p astersql-pd-schedule-operator --locked` | 0 | 12通过、0失败 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1375通过、0失败、1既有忽略 |
| `git diff --check` | 0 | 通过 |

[readonly-validation.py](logs/task_017/readonly-validation.py)和
[reuse-go/go](logs/task_017/reuse-go/go)复用批次14留下的真实etcd二进制，仅接受
既有etcd fixture构建参数并复制二进制，拒绝执行其它Go命令。来源、内容哈希、
调用记录保存在[provenance](logs/task_017/fixture-reuse-provenance.json)和
[go-reuse.jsonl](logs/task_017/go-reuse.jsonl)，没有跳过Rust存储测试，
不是operator Go oracle。适用于当前默认features；成本领域本身没有classic/nextgen分支。

所有Cargo阶段经[slot-run.py](logs/task_017/slot-run.py)原子mkdir动态领取槽位。
本次各阶段实际均为槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
子进程结束后释放自己的锁，保留缓存；信号/错误路径也等待子进程并清理。
完整[verification.json](logs/task_017/verification.json)记录Go/模块文件全量hash
和Git状态未变、两份只读plan未变、来源验证、fixture hash、锁变更范围与槽位释放。

仅提交此任务的Rust实现、测试、接线、说明、fixture和验证证据。
排除开始时已有任务10–16删除。完成后删除任务17编号文件；不修改只读plan或原主题。

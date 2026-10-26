# 任务085：热点调度配置契约

依赖84已核验：提交 `92512140b`、[task-084](task-084.md)、真实 SlowTrendScheduler
及 MemoryStore/StorageEndpoint API；前序5个聚焦、76个包级、1625个workspace测试通过。
已读取整体/包级只读计划及 oracle-index。按用户后续明确授权，本任务采用
**只读 Go 源码/测试推导 + Rust 真实行为和故障验证**，替代缺失的 Go 运行 oracle。
**未运行热点 Go 测试，没有 Go 运行输出或运行差分。**

实现：[hot_config.rs](../../../pkg/schedule/schedulers/hot_config.rs)，
[领域说明](../../../pkg/schedule/schedulers/hot_config.rs.md)，
[测试](../../../pkg/schedule/schedulers/task_085_test.rs)。公开 API：HotRegionParams、
HotRegionConfig::{new,params,set_support,valid_config,encode,update,update_json,reload}、
adjust_hot_priorities、HotPriorityScope、HotConfigUpdate、HotConfigError。
原始参数与有效配置分开；查询支持变化不改写原始配置；持久化失败保留已发布内存值；
reload 校验后发布、失败保留原值。历史迁移、versioninfo、HTTP 状态映射和 solver 不在范围。
新增唯一依赖为既有 canonical typeutil（duration）；Cargo.lock 仅加入该 workspace 路径依赖，
离线生成时出现的 pulldown-cmark 版本漂移已恢复，未升级远程依赖。

## 期望与失败证据

[provenance](logs/task_085/provenance.json) 记录当前来源提交、6个 Go 文件的 SHA-256、
fixture 摘要；这些源码与计划基准 `836fdc7cf3ef34f7eabc9d051418995bb97efabe` 相同。
[fixture](fixtures/task_085.json) 固定96组输入/输出：3种scope × 4组支持标志 × 8组原列表，
覆盖合法/非法/空/重复/多维及 CPU/query 回退，并逐字段冻结所有默认参数。
Go TestAdjustPrioritiesConfigCPUFallback 的5个 read 案例有只读期望；其余期望来自
adjustPrioritiesConfig、validateLocked、getValidConf、handleSetConfig、ReloadConfig 的源码分支，
不把该 Go 测试名当成所有边界/故障的运行证据。

`red-api` 为缺 API 编译失败，退出101，不算行为红灯。
`red-behavior` 将 query fallback 临时改成 CPU，96组 fixture 在
query=true/cpu=false/origins=[] 断言失败（实际cpu/byte，期望query/byte），
effective snapshot 的同一回退断言也失败：2通过/2失败，退出101。
finally 逐字节恢复生产文件后，同一4个测试全部通过，无忽略。

## 行为与故障

- `task_085_behavior_cases`：96/96冻结案例，比较维度和值的顺序。
- `task_085_validation_and_effective_snapshot`：全部默认字段，11类拒绝原因、
  24组 rank(v1/v2/空)/forbid/threshold闭区间边界、3组空/三/四维列表、
  CPU/query 标志更新、raw/effective 分离、zombie30s/180s、history300s/30s及负值、
  字符串布尔、Go duration编码/解码、TiFlash默认开关及0.1修正常量。
- `task_085_update_reload_and_persist_failure`：真实 MemoryStore 更新/保存/rebuild、
  pending weight、rank、forbid write、TiFlash/strict关闭、history1.5s；
  保存失败内存保留而旧存储不变、无变化不执行失败I/O、read失败不发布、reload恢复旧值；
  12组非法更新均检查内存与存储不变，4组非法重载检查内存不变；
  unknown/partial/null/case-insensitive字段、empty priorities有效回退、支持标志不持久化、
  reload不改base disabled、typed NaN拒绝。
- `task_085_cancelled_io_keeps_published_config`：真实事务边界pending后取消future，
  活动配置保留3，存储仍为2，解除故障后reload恢复2。无泄漏后台任务。

MetaStore故障仅置于read/transact I/O边界，其余委派真实MemoryStore。
无时间/随机输入需要归一化；JSON对象键顺序忽略，浮点按数值、duration按Go文本、
优先级按有序数组比较。null优先级规范化为空列表，重编码为[]；有效回退一致。
未忽略错误、参数或状态差异。CPU/query版本检测留在宿主已有边界，领域接收布尔结果；
solver应用TiFlash修正归后续任务。本任务无Go/failpoint写入。

## 验证

每条确切命令、退出码、环境及实际槽位保存在 `logs/task_085/*.json`；stdout/stderr
合并保存于同名 `.log.gz`。[run.py](logs/task_085/run.py) 每阶段按原子mkdir动态领取槽位，
保存plan/task/session/PID，失败或中断等待子进程后清理自身锁，缓存保留。
本任务实际领取**槽位1**，绝对 `CARGO_TARGET_DIR` 为
`/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。

| 确切命令 | 退出码 | 有效结果 |
| --- | --- | --- |
| `cargo generate-lockfile --offline` | 0 | 只保留typeutil路径依赖，远程版本未变 |
| `cargo fmt --all` | 0 | 仅本任务Rust差异 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_085 -- --nocapture` | 0 | 4通过/0失败/0忽略，96固定优先级案例及上述边界/故障 |
| `cargo test -p astersql-pd-schedule-schedulers --locked` | 0 | 80通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `CARGO_BUILD_JOBS=1 cargo check --workspace --locked` | 0 | 通过；已有cpu_usage弃用警告 |
| `CARGO_BUILD_JOBS=1 RUST_TEST_THREADS=2 cargo test --workspace --locked` | 0 | 1629通过/0失败/1既有忽略，含2个文档测试 |

[计数](logs/task_085/counts.json) 逐组汇总真实输出；既有忽略为tso_processes。
[red-api](logs/task_085/red-api.log.gz)、[red-behavior](logs/task_085/red-behavior.log.gz)、
[聚焦](logs/task_085/green.log.gz)、[包级](logs/task_085/package.log.gz)、
[workspace](logs/task_085/workspace-test.log.gz) 保存完整输出，不以零测试组替代有效行为计数。

`python3 docs/rust/schedule-execution/logs/task_085/audit.py` 与 `git diff --check` 均退出0。
[audit](logs/task_085/audit.json) 比较1119个Go/模块/plan文件前后SHA-256，全部一致，
含untracked文件扫描；[开始快照](logs/task_085/before.json.gz)及初始Git状态保留。
只读审查workspace既有etcd fixture构建/运行路径：复用已有Go源码，binary与WAL落在槽位，
不注入failpoint、不执行Go测试或改写Go/模块；proto本轮复用既有锁定生成缓存。
已审查生产/测试/文档/fixture接线及相对链接，无新增Go差异、无修改两级plan或来源主题。
其它任务78/80/81/82/83/84的已有删除排除在本次提交之外。
所有必需验证通过，实际槽位1自身锁已释放，编译缓存保留。状态：已完成。

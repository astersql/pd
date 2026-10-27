# 任务151：区域扰动配置传输适配

状态：已完成。前置150提交 `88c1f937c` 及 [task-150](task-150.md) 的
全量验证和API已核实；没有新增crate、feature或修改Cargo依赖。

## 来源与准入

用户明确授权只读Go源码/测试推导加Rust真实行为和故障验证替代缺失Go运行oracle。
本任务没有运行Go，不声称取得Go运行差分。oracle-index中的source_assertion
没有伪填为runtime_fixture。[provenance](logs/task_151/provenance.json)记录来源commit、
源码哈希、固定fixture和oracle-index哈希；Go仓库来源均与
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 字节相同。

`shuffle_region.go`有自己的ServeHTTP，委托 `shuffle_region_config.go`，因此不适用
BaseScheduler的not implements。只读 `TestShuffleRegion` 覆盖空集群和REGION操作，
`TestShuffleRegionRole`覆盖follower/learner搬迁；它们直接修改配置，不覆盖HTTP错误、
持久化失败或JSON响应。没有把测试名称当运行证据，也没有裸跑需failpoint的Go测试。
额外只读检查实际依赖mux v1.7.4和render v1.0.1，确认路由清理、方法和响应编码。

[HTTP fixture](fixtures/task_151.json)冻结41条源码推导场景；
[调度fixture](fixtures/task_102.json)复用既有26条源码推导输入/期望。
JSON对象key顺序允许不同；JSON换行及GET数组响应逐字节验证，数组顺序、null/[]、字符串、数值Offset、状态、header精确比较。
HEAD在直接handler层验证405；完整server/api装配不在本任务。
调度沿用task102明确边界：store按ID排序，shuffle正/反序，随机首/末目标，
固定10^12ns、单region [a,b)、peer1–3、新peer10，分别运行conf-change v1/v2。
请求体I/O错误保留internal/code/data及非空stack；stack内容随Rust调用点变化，不作
Go栈字节相等宣称。受控MetaStore I/O错误使用真实后端的Unavailable错误文本。

## 实现及测试驱动

生产入口 [schedule_admin_shuffle_region.rs](../../../pkg/server/schedule_admin_shuffle_region.rs)
实现GET /list、GET /roles、POST /roles、404/405、清理路径301和解码路径匹配。
同名说明文档与独立测试显式注册。canonical ShuffleRegionScheduler增加
`update_optional_roles`保留HTTP null，原有update_roles委托同一路径；所有角色先验证，
持久化成功后发布，保存失败保留旧内存/存储，范围不改。

[红测](logs/task_151/red.json)以错误候选501运行同一真实HTTP断言，退出101，
0通过/1失败，明确在501与200状态断言失败。
[初次绿测](logs/task_151/green.json)、[扩展测试](logs/task_151/expanded.json)
为中间证据，随后依照实际render源码补正纯文本charset大写和JSON换行，
以最终日志及最终fixture为验收依据。
[编码红测](logs/task_151/encoding-red.json)退出101，3通过/1失败，
明确由新增的IndentJSON换行断言检出缺失换行；修正后重新执行全部验证。

[测试](../../../pkg/server/schedule_admin_shuffle_region_test.rs)：

- `task_151_behavior_cases`：41条方法/路径/请求场景。查询、编码路径、清理路径、
  空/错误JSON、类型错误及Offset、非法/重复/空/null角色、状态/header/body、
  配置范围保持、每条持久化及GET读回。
- `task_151_schedule_after_update`：26场景×2种conf-change模式=52次真实调度，
  HTTP更新角色后从真实storage reload再调度；有效角色更新前先验证空角色无操作。
  非法存储角色HTTP拒绝且不改旧值。覆盖leader/follower/learner、无角色、pending/down、
  replica不足、无region/target、special/offline/disconnected/限额、范围、placement、
  分配失败和随机选项。核对目标、步骤、kind、Low、heartbeat命令、influence执行、
  confver、逐步骤应用及最终完成；完整使用真实core索引、过滤、placement和builder。
- `task_151_persistence_faults`：3次实际保存I/O失败均500并回滚；非法角色在故障时
  仍400；恢复后4次更新/reload含null、[]、learner和重复写，验证持久化与内存一致。
- `task_151_body_faults`：4条不应读body的分支使用poll即panic的流；POST已收到完整
  JSON后仍出现实际流错误，返回internal 500，内存和持久化保持不变。

## 最终验证与审计

[verify.py](logs/task_151/verify.py)使用原子mkdir从1–10动态领取槽位，记录任务、
计划、会话和PID；所有阶段实际领取 **槽位1**，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
默认features，CARGO_BUILD_JOBS=4、RUST_TEST_THREADS=2。
子进程正常/失败/中断均等待结束后释放自有锁，保留缓存；最终自有锁已释放。
[只读Go拦截器](logs/task_151/reuse-go/go)拒绝实际Go命令，复用已校验哈希的既有etcd
二进制和kvproto ZIP；[复用记录](logs/task_151/go-reuse.jsonl)保存实际调用及摘要。
没有Go构建/测试、生成、make、failpoint或模块写入。

[最终命令、退出码及原始日志](logs/task_151/verified.json)、
[有效测试汇总](logs/task_151/verified-summary.json)：

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 仅本任务相关Rust格式变更 |
| `cargo test -p astersql-pd-server --locked --lib task_151 -- --nocapture` | 0 | 4通过、0失败、0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 106通过、0失败、0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 通过 |
| `cargo fmt --all -- --check` | 0 | 通过 |
| `cargo check --workspace --locked` | 0 | 通过 |
| `cargo test --workspace --locked` | 0 | 1906通过、0失败、1既有忽略 |

仅出现既有cpu_usage弃用warning；未添加忽略或跳过必需验证。
[审计脚本](logs/task_151/audit.py)及[结果](logs/task_151/audit.json)对比
[开始快照](logs/task_151/baseline.json)，确认全部Go/模块文件（含untracked）及两份
只读plan集合/内容哈希、Go Git状态不变；日志哈希及Markdown链接有效，
git diff --check退出0。Cargo清单/锁没有修改。

只提交本任务生产适配、canonical null角色更新、测试、说明及证据；其它任务既有
删除全部排除。验收后删除151编号任务文件，不修改plan或原始主题。

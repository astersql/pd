# 任务057：可疑范围复查

生产路径：[suspect_ranges.rs](../../../pkg/schedule/checker/suspect_ranges.rs)，
[边界说明](../../../pkg/schedule/checker/suspect_ranges.rs.md)。
API：`SuspectRanges::{new,add,pop,clear,check_once,run}`，
`SUSPECT_REGION_LIMIT=1024`、`SUSPECT_RANGE_INTERVAL=100ms`。

依赖批次56提交 `09785453c9bee706c10d5c88d8a8f1cf0b799691`，
[前置证据](task-056.md) 已核实。复用 canonical BasicCluster、TTLString、
CheckerDispatcher 的 pending TTL，以及真实 Patrol/check_patrol_region/admission 链路。
只读整体/包级 plan.md、oracle-index.json、ADR-001 和移植指南已阅读。

依据用户后续授权，采用 **只读 Go 源码推导 + Rust 行为验证**，没有运行本任务 Go oracle，
不声称 Go/Rust 运行差分。Go 输出和输出哈希均为 null。
[来源记录](logs/task_057/provenance.json)保存实际来源提交、Go 源码 SHA256、fixture SHA256；
[fixture](fixtures/task_057.json)冻结 7 个扫描表项和 9 个附加行为故事。
来源为 `checker_controller.go:470–533`、`cache/ttl.go` 和 `keyutil/util.go`。

队列只按精确范围标识去重，重叠范围不做 union，pop 次序未定义。
每 tick 弹出一个有效范围，真实 registry 扫描最多1024个 region，有限 end 尚未扫描完则补排余段，
空扫描直接消耗范围，ID 进入巡逻共享 pending TTL 且绕过容量限制。
保留 requested end 的字节比较：空 end 不补排下一批。
canonical core 用空 Vec 表达 Go nil terminal end，不保留显式 nonnil 空 Go 切片的指针差异；
这是现有领域表示边界，已在 fixture 中逐字段说明。错误、操作、目标和ID不作归一化。

`run` callback 在每100ms tick获取宿主现有锁并调用 `check_once`，允许心跳在tick间修改registry；
callback错误原样返回。停止取消后等待在途callback结束，未进入callback时立即唤醒退出。
宿主持有TTL生命周期并在worker退出后shutdown，不建立私有runtime或另一份region/pending状态。

[独立测试](../../../pkg/schedule/checker/task_057_test.rs)包含6个有效测试：

1. `task_057_behavior_cases`：7个冻结扫描表项，覆盖1024上限、有限/空end、terminal、
   空registry、缺失region、精确边界；检查pending数量及补排起点。
2. `task_057_identity_clear_and_expiration`：重复范围合并、重叠范围独立、clear、180秒TTL到期。
3. `task_057_tick_and_stop`：99ms尚未执行、101ms已扫描，取消后保留未处理队列，预取消不执行callback。
4. `task_057_continuation_and_shared_patrol`：真实1030个region按1024+6续扫；重复ID仍只有一项；
   clear范围不清pending；复用批次56真实宿主装配，范围入pending后由Patrol检查并产生一个
   promote-learner等待操作，pending随真实admission转移。复用宿主还断言操作的PromoteLearner步骤。
5. `task_057_host_fault_and_inflight_join`：外部宿主边界故障原样返回；取消期间等待在途callback释放。
6. `task_057_pending_cap_bypass`：共享pending已有100002项后仍能添加region 1，最终100003项。

最初先注册测试，以缺失扫描行为的空返回实现执行同一聚焦命令：
[red.log](logs/task_057/red.log)退出101，1通过/2失败；实际断言为0与1024不符、空pending与[1]不符。
这是真实行为失败，不是缺包或未注册。后续集成fixture出现一次辅助方法usize/u64编译错误，
[focused.log](logs/task_057/focused.log)保留诊断，修正辅助返回类型后通过。
未修改Go、生产前置checker或其它任务的业务逻辑。`task_056_test.rs`仅开放最小测试fixture入口，
避免复制完整真实宿主。keyutil从dev dependency移至production dependency，Cargo.lock无变化。


## 最终验证与审计

状态：已完成。

所有Cargo阶段由 [run.py](logs/task_057/run.py)按原子mkdir动态领取共享槽位，
记录任务、会话和PID，失败/中断时终止并等待子进程后释放自身锁，保留缓存。
本任务所有阶段实际领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
全部子进程已结束，自有槽位锁已释放。

精确命令、退出码与环境见 [commands.json](logs/task_057/commands.json)，
聚焦/包级/workspace测试数量见 [test-summary.json](logs/task_057/test-summary.json)。
以下最终命令全部退出0：

| 命令 | 有效结果 |
| --- | --- |
| `cargo fmt --all` | 通过 |
| `cargo test -p astersql-pd-schedule-checker --locked --lib task_057 -- --nocapture` | 6通过，0失败；7个扫描表项、9个附加行为故事 |
| `cargo test -p astersql-pd-schedule-checker --locked` | 58通过，0失败 |
| `cargo metadata --format-version 1 --no-deps` | 通过 |
| `cargo fmt --all -- --check` | 通过 |
| `cargo check --workspace --locked` | 通过，仅既有server废弃字段告警 |
| `RUST_TEST_THREADS=2 cargo test --workspace --locked` | 1516通过，0失败，1既有忽略 |

全量计数包含2个compile-fail文档测试；既有忽略为`tso_processes`。
[全量日志](logs/task_057/workspace-test.log)与[最终聚焦日志](logs/task_057/focused-provenance.log)
可独立审查。运行日志只清理行末空白与末尾空行，未改写测试、错误或状态输出。
workspace既有etcd fixture运行不代表取得本任务Go调度oracle。

[audit.py](logs/task_057/audit.py)与[结果](logs/task_057/audit.json)核对1101个Go/模块文件
清单及SHA256完全一致，Go Git状态为空，两份只读plan.md与HEAD一致，文档链接有效，
`git diff --check`通过。提交仅包含本任务实现/说明/测试/fixture/证据、必要模块接线和测试宿主复用，
排除任务开始前已有的其它编号任务删除。完成任务文件按授权删除，不修改来源主题或plan.md。

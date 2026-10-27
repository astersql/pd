# 任务142：停止节点驱逐配置传输适配

依赖141提交 `da39bcc42` 及[完整验收证据](task-141.md)已核实，复用082停止节点
领域状态机、core/placement/operator/storage，不修改两级plan或原始主题。

## 契约来源

依据用户后续明确授权，使用**只读 Go 源码/测试推导 + Rust 真实行为和故障验证**。
没有运行 Go 测试、没有 Go 运行输出，不主张运行差分。来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`，来源文件与基准一致；源码、索引及
[fixture](fixtures/task_142.json) SHA256 见[provenance](logs/task_142/provenance.json)。

核实 evict_stopping_store.go 的独立 ServeHTTP、newEvictStoppingStoreHandler、
updateConfig、listConfig、persistLocked；config.go 的 Disabled 嵌入字段和
apiutil.ReadJSONRespondError。BaseScheduler.ServeHTTP 虽返回 `not implements`，
此 scheduler 已覆盖该方法，不采用不支持分支。

只读核实 TestEvictStoppingStoreTestSuite：TestEvictStoppingStore 覆盖停止节点驱逐
与禁止均衡到停止节点；Recovery 覆盖解除停止后无驱逐及恢复均衡；RemovedStoppingStore
覆盖墓碑节点清理。它们没有 HTTP 配置断言，不能将测试名当作 HTTP 运行证据。
本任务 HTTP 期望来自 handler 源码分支，未裸跑需 failpoint 的 Go 测试。

JSON 仅归一化键序、缩进、尾随换行；错误消息/状态/data 不忽略。真实读取故障
栈帧不跨语言比较，存储错误为真实 MetaStore 错误。时钟固定10^12ns、候选索引递增、
不打乱，未声称 Go map/随机顺序一致。未穷举任意 JSON 解析诊断；Gorilla 路径清理/
重定向与完整 server/api 路由装配属于后续主题。

## 实现和行为验证

生产入口 [serve](../../../pkg/server/schedule_admin_evict_stopping_store.rs) 处理相对
POST `/config` 与 GET `/list`，借用宿主 canonical EvictStoppingScheduler、
StorageEndpoint、ClusterPaths、disabled。宿主串行驱动配置和调度，借用覆盖 I/O。
无新增依赖、feature、runtime 或全局路由。[接口说明](../../../pkg/server/schedule_admin_evict_stopping_store.rs.md)。

batch 为数值 [1,10]，小数截断；缺失/null/未知字段仍保存，重复字段最后值生效。
最小领域桥接 `EvictStoppingScheduler::update_http` 保留驱逐集合，省略 batch 时可
保存历史加载值100，保存宿主 disabled 后才发布配置。请求 disabled 不覆盖宿主；
list 构造零值嵌入 base，输出 disabled=false。保存失败回滚内存及持久化快照。
既有 observe/schedule 的 disabled 生命周期仍由宿主装配负责。

[独立注册测试](../../../pkg/server/schedule_admin_evict_stopping_store_test.rs)共4函数，47组：

- task_142_behavior_cases：31个固定 HTTP 案例，默认/最终list、方法/路径错误、
  数值类型、0/-1/11/10.1/0.9拒绝、1/10成功、2.9截断、null/未知/大小写/
  重复键，JSON错误完整响应及拒绝后状态保持。
- task_142_faults_and_body_errors：10组存储失败/重试、6种非法根JSON与完整/截断
  JSON后真实读取失败。只在 MemoryStore 的 MetaStore I/O 边界注入失败，验证
  500、错误内容、未应用更新、旧持久化快照、重试后真实状态。
- task_142_disabled_and_legacy_batch：2组宿主 disabled=true持久化、请求false忽略、
  list=false；保留已有多元素驱逐集合及历史batch100。
- task_142_updated_scheduling：4组 HTTP 更新为1.9/3.9/5.9/10后，batch1/3/5/10
  生成共19个真实操作器；真实存储恢复/list/驱逐引用、阻塞目标、URGENT/LEADER、
  TransferLeader 1→2/3、confver、影响量、v1/v2命令、执行完成、停止恢复均核实。

## 红绿与验证环境

[red](logs/task_142/red.json)：先fmt退出0，再实际运行1项同名行为测试；默认list
返回404、期望200，断言失败exit101。不是缺模块/零测试。实现后首次编译发现错误类型
转换不匹配，保存于[green](logs/task_142/green.json)，已修正，不当作行为红灯。
[green2](logs/task_142/green2.json)4项测试/47组全部通过。

[验证脚本](logs/task_142/verify.py)每阶段通过原子mkdir动态领取共享槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=2；所有 Cargo/子进程继承槽位。
owner记录计划/任务/会话/PID，失败/中断等待子进程组退出再释放锁，保留缓存。
Rust修改后先 cargo fmt --all，差异仅本任务文件。

沿用141安全复用流程，源二进制 rust-slot-1/rust-etcd-fixture-2647 SHA256
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`。
当前槽位硬链接 task142-existing-etcd；[Go拦截器](logs/task_142/reuse-go/go)只允许
校验后原子硬链接复用既有fixture的命令形式，拒绝实际Go命令。不是新建Go oracle，
不写Go/模块源码、不启停failpoint。复用活动记录在 go-reuse.jsonl。

[审计](logs/task_142/audit.json)包含1101份Go/模块文件的集合、SHA256和Git状态，
含未跟踪源文件；排除target/.git/.tools缓存目录，两级只读plan一并核实。
初始完整快照保存在 before.json.gz。Cargo清单/锁未变化；Markdown链接已核对，
git diff --check退出0。其它任务已有删除不属于本次提交。

## 最终结果：已完成

[完整命令/退出码/原始日志](logs/task_142/final.json)及
[测试汇总](logs/task_142/final-summary.json)如下：

| 命令 | 退出码 | 有效测试/结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-server --locked --lib task_142 -- --nocapture` | 0 | 4通过，47组，0失败/忽略；green2 |
| `cargo test -p astersql-pd-server --locked` | 0 | 75单元+1集成=76通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | 全workspace检查 |
| `cargo test --workspace --locked` | 0 | 1876通过、0失败、1既有忽略 |

最终验证后未修改Rust代码。所有阶段实际槽位1，绝对CARGO_TARGET_DIR见上文；
全部槽位子进程已结束，锁目录已释放，缓存保留。任务142完成后删除编号任务文件，
稳定证据保留本页，不修改plan或原始主题，不将其它任务删除纳入提交。

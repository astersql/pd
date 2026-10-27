# 任务143：热点授予配置传输适配

依赖142提交 `2355f6cd7` 与[验收证据](task-142.md)已核实，复用096的
GrantHotScheduler、真实core/placement/operator/StorageEndpoint，不修改Go或两级plan。

## 契约与边界

依据用户后续明确授权，采用**只读 Go 源码/测试推导 + Rust 真实行为和故障验证**。
没有运行 Go 测试或生成运行oracle，不主张Go/Rust运行差分。来源commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`，源码与此基准一致；
[fixture](fixtures/task_143.json)、源码和oracle-index的SHA256见
[provenance](logs/task_143/provenance.json)。索引不代表此端点已有Go运行输出。

只读核实grant_hot_region.go的ServeHTTP、newGrantHotRegionHandler、updateConfig、
listConfig、setStore、persist；base_scheduler.go默认不支持分支被该scheduler覆盖。
config.go中的schedulerConfig接口没有导出JSON字段，因此list/persist无disabled字段。
apiutil.ReadJSONRespondError与pingcap/errors的GetMsg/MarshalJSON决定错误对象结构。
TestGrantHotRegionUpdateConfigWithInvalidLeaderIDType只断言数值leader被400拒绝；
TestGrantHotRegionTransferLeaderInLimit只断言leader-in预算耗尽时无操作器，恢复后
1→2的LEADER操作器；未将这些名字冒充HTTP完整覆盖或运行证据。既有task096
行为测试涵盖真实leader-in预算，随workspace全量测试重跑。

生产入口 [serve](../../../pkg/server/schedule_admin_grant_hot_region.rs)借用宿主
GrantHotScheduler、StorageEndpoint、ClusterPaths，处理相对POST `/config`、GET
`/list`。方法错误405空体，无Allow；未知路径404文本。成功更新返回JSON null；
list默认store-id为空数组。校验顺序为store字符串类型、逐项ParseUint十进制u64、
leader字符串类型、leader ParseUint。错误对象保留原始消息模板及RFC code。
合法输入交给已有update_config；保留顺序与重复，leader不在列表时追加。
保存失败保留新store列表、清零leader，旧持久化快照可reload恢复。
无新增依赖/feature/runtime；[接口说明](../../../pkg/server/schedule_admin_grant_hot_region.rs.md)。

JSON只归一化键序、缩进、尾随换行，不忽略状态、错误或字段。真实读取故障的栈
不跨语言比较；存储错误来自真实MetaStore。时钟固定100ns、随机目标固定索引0；
不声称Go map迭代或随机分布相同。未穷举所有JSON解析诊断；Gorilla路径清理/
重定向与完整server/api装配属于后续主题。

## 测试与红绿证据

[独立注册测试](../../../pkg/server/schedule_admin_grant_hot_region_test.rs)共3函数、66组：

- task_143_behavior_cases：48个固定HTTP案例，方法/路径、默认/更新后list、
  缺失/null/数值/数组/大小写键、非法ID、u64越界/最大值、零/前导零、重复ID、
  自动追加leader、重复JSON键、未知字段、非法根JSON完整响应及拒绝后状态保持。
- task_143_faults_and_body_errors：10组，真实MemoryStore I/O失败、500错误、
  leader清零/新列表保留、list可见、reload失败与恢复、旧磁盘值、重试与恢复；
  6种非法JSON及完整/截断JSON后的真实流读取失败，断言状态未变。
- task_143_updated_scheduling：8组，HTTP分别配置leader2/3、peer4/5，存储恢复
  后经真实dispatch生成8个操作器。验证目标、HIGH、LEADER/REGION kind、步骤、
  confver、v1/v2命令及peer移动影响量；leader目标暂停时无目的地，解除后继续。

[red](logs/task_143/red.json)记录测试引入时误指向相邻模块的编译错误，修正后
[red2](logs/task_143/red2.json)实际运行同名行为测试，默认list返回404、期望200，
断言失败exit101。不是零测试或缺模块。green记录测试代码的两个类型错误；
修正后[green2](logs/task_143/green2.json)3测试/60组通过。补充6个完整JSON错误
fixture后，最终聚焦验证3测试/66组通过，完整日志保留。

## 验证环境

[验证脚本](logs/task_143/verify.py)每阶段原子mkdir动态领取共享槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
CARGO_BUILD_JOBS=2、RUST_TEST_THREADS=2，Cargo及子进程继承设置。owner记录计划/
任务/会话/PID，失败和中断等待进程组结束再释放锁，缓存保留。Rust修改后先运行
cargo fmt --all，格式化差异仅本任务新增文件及lib注册。

沿用142只读复用流程，既有etcd I/O fixture SHA256
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`，
在当前槽位硬链接为task143-existing-etcd。
[Go拦截器](logs/task_143/reuse-go/go)只允许校验后硬链接复用这一既有二进制，
拒绝所有实际Go命令；不创建Go oracle、不生成或修改Go源码/模块、不启停failpoint。
复用活动保存于go-reuse.jsonl。


## 最终结果：已完成

[全部命令、退出码和原始日志](logs/task_143/final.json)、
[测试汇总](logs/task_143/final-summary.json)：

| 命令 | 退出码 | 有效测试/结果 |
| --- | --- | --- |
| `cargo test -p astersql-pd-server --locked --lib task_143 -- --nocapture` | 0 | 3通过，66组，0失败/忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 78单元+1集成=79通过，0失败/忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | workspace元数据 |
| `cargo fmt --all -- --check` | 0 | 格式检查 |
| `cargo check --workspace --locked` | 0 | workspace检查 |
| `cargo test --workspace --locked` | 0 | 1879通过，0失败，1既有忽略 |

Rust最终验证后未变更。所有阶段实际槽位1，CARGO_TARGET_DIR为上述绝对路径。
最终脚本在所有Cargo子进程结束、结果保存后，删除owner时遇到FileNotFoundError，
wrapper退出1；不是Cargo命令失败。检查发现整个target/rust-slot-locks目录已不存在，
没有残余槽位子进程，缓存仍在；原因未知，本任务没有删除锁父目录的命令。
[收尾异常记录](logs/task_143/final-cleanup.json)保留此事实，未声称最终脚本成功
自行释放已消失的锁。随后仅增强脚本的owner核对与已消失锁记录，不改变Rust。

[审计脚本](logs/task_143/audit.py)及[结果](logs/task_143/audit.json)核实1101份
Go/模块文件集合、内容哈希与Git状态不变，含untracked源码；两级只读plan、
Cargo清单/锁保持原样，Markdown链接有效，git diff --check退出0。初始保护文件
快照见before.json.gz。仅提交本任务实现、测试、接线和证据，其它任务已有删除排除。
验收完成后删除143编号任务文件；不修改两级plan或原始主题。

# 任务138：区域均衡配置传输适配验收

状态：已完成。前置137提交 `84ae645e9` 与[验收证据](task-137.md)已核实；
不存在以任务文件删除替代依赖验证的情况。

## 来源与契约

按用户后续授权采用**只读 Go 源码/测试推导 + Rust 真实行为和故障验证**，
替代缺失的 Go 运行 oracle；没有执行或宣称 Go 运行差分。
[provenance](logs/task_138/provenance.json)冻结来源 commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe`、三份Go来源哈希、
前置137/72证据、72 fixture、本任务[fixture](fixtures/task_138.json)与oracle索引哈希。
三份Go来源与基准相同；Go binary/output SHA256明确为null。

Go `balanceRegionScheduler` 内嵌 `BaseScheduler`，没有覆盖 `ServeHTTP`；
基类写入 `not implements`，默认200，隐式text/plain UTF-8内容类型。
`balance_region.go` 中 ReloadConfig 仍为 TODO，不能沿用 leader 的动态配置接口。
HTTP没有JSON响应，错误JSON也保持同一明文响应；逐字节比较body与内容类型，
没有忽略错误或归一化状态差异。HEAD测试针对handler调用，实际线上body抑制由
HTTP宿主完成；主题7全局路由装配不在本任务。

指定Go测试的实际覆盖已只读核实：TestInfluenceAmp比较在途影响边界；
TestBalanceRegionOrdinaryMoveNotBlockedByCandidateSize比较普通候选允许搬迁；
TestBalanceRegionLargeCandidateDoesNotOvershoot比较大候选过冲拒绝；
TestSingleRegionOnLargeEmptyDiskDoesNotMigrate比较6TiB单非空region评分；
TestBalanceRegionRealScheduleDoesNotMoveSoleRegion双向真实Schedule重复拒绝迁移。
这些测试没有验证HTTP配置端点，不将其名称当作Go运行证据。
本次重新运行任务072的3个Rust测试，覆盖其22个真实调度场景、5个solver边界
和配置/限额断言，保留既有领域回归；具体逐案例输出见最终日志。

## 生产入口与验证案例

新增 [serve](../../../pkg/server/schedule_admin_balance_region.rs)，
在canonical `astersql-pd-server`里显式注册，对真实`BalanceRegionScheduler`
只读借用，返回完整的BaseScheduler不支持响应；
[实现说明](../../../pkg/server/schedule_admin_balance_region.rs.md)记录边界。
没有新增crate、依赖、领域状态、配置持久化路径或Go改动。

[测试](../../../pkg/server/schedule_admin_balance_region_test.rs)包含3个函数、18个有效案例：

- `task_138_behavior_cases`：15项HTTP。GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS，
  `/config`、`/list`、根路径、未知路径、query、正常ranges、非法JSON、null、
  ranges类型错误和非法hex；全部200、原始明文和content-type一致，非默认配置保持不变。
- `task_138_body_io_failure`：1项外部请求体故障。body的poll直接panic，
  只有完全不读取故障body才能完成响应；配置保持不变。
- `task_138_schedule_after_unsupported_update`：2阶段真实调度。在HTTP POST企图
  将ranges切到region以外之前和之后，均生成region1的store1→store4操作，
  验证AddLearner目标4/peer1000、RemovePeer来源1、输入pending influence不变，
  MemoryStore中的scheduler配置为空。源大小288、目标0、candidate96的允许搬迁
  契约来自Go ordinary回归和任务072已冻结fixture。时间和随机输入显式固定。

使用真实BasicCluster、StoreInfo、RegionInfo、RuleManager、RegionLabeler、
StatisticsSnapshot、OpInfluence、MemoryStore/StorageEndpoint和操作工厂。
本调度器无配置写入入口，因此“配置持久化失败后更新”和“动态更新后reload”
不适用；HTTP接口不接受storage参数，不制造新的配置更新能力。

## 红绿验证与仓库检查

[red](logs/task_138/red.json)保存初次 `cargo fmt --all` 退出0和聚焦测试退出101：
2个测试均因错误404与冻结200断言失败，证明同一测试能识别错误行为。
[green](logs/task_138/green.json)保留追加真实调度测试时StorageEndpoint不可Clone的
编译接线错误；[setup-correction](logs/task_138/setup-correction.json)保留测试遗漏
max_replicas=3引起的0操作断言失败。这两次不计为生产行为红验证；修正测试装配后
完整运行最终验证，未修改调度算法。

[final](logs/task_138/final.json)保存确切命令、退出码、测试摘要、完整合并stdout/stderr
的gzip日志及SHA256。[audit](logs/task_138/audit.json)保存只读文件和证据审计。

| 命令 | 退出码与有效测试 |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo test -p astersql-pd-server --locked --lib task_138 -- --nocapture` | 0；3通过、0失败、18有效案例 |
| `cargo test -p astersql-pd-server --locked` | 0；58通过、0失败 |
| `cargo test -p astersql-pd-schedule-schedulers --locked --lib task_072 -- --nocapture` | 0；3通过、0失败 |
| `cargo metadata --format-version 1 --no-deps` | 0 |
| `cargo fmt --all -- --check` | 0 |
| `cargo check --workspace --locked` | 0 |
| `cargo test --workspace --locked` | 0；1858通过、0失败、1既有忽略 |

[槽位脚本](logs/task_138/verify.py)在每个阶段原子mkdir动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
owner记录计划、任务、会话和PID；正常、异常和信号中断先join所有子进程组，
再删除自己的owner/rmdir锁；缓存保留。所有Cargo与子进程继承绝对目录。

延续137的[Go拦截器](logs/task_138/reuse-go/go)仅复制任务030已保存的etcd二进制，
SHA256 `57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`，
拒绝所有实际Go命令；[复制记录](logs/task_138/go-reuse.jsonl)保存实际调用。
没有运行Go测试、make、failpoint启停、生成或模块整理。

[审计脚本](logs/task_138/audit.py)核对开始/结束所有1101份Go与模块文件集合、内容、
Git状态、两级只读plan.md、依赖/来源/fixture/log哈希、Markdown链接和git diff检查。
不改写来源主题，既有其它任务删除排除在本任务提交之外。

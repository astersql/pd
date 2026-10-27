# 任务144：热点均衡配置传输适配

依赖143提交 `51bf9531a` 与[证据](task-143.md)已核实，使用既有
`HotRegionConfig`、`HotScheduler`、真实core/operator/统计与MetaStore契约。
不修改Go或两级plan；完整server/api装配属于主题7。

## 契约来源

用户后续明确授权以**只读Go源码/测试推导 + Rust真实行为和故障验证**替代缺失
Go运行oracle。适用模式为默认Cargo features下的scheduler局部接口，宿主提供
classic/nextgen状态及版本能力，不新增feature。没有运行Go测试，不声称Go/Rust运行差分。基准commit
`836fdc7cf3ef34f7eabc9d051418995bb97efabe` 的所用Go来源与当前源码一致。
[provenance](logs/task_144/provenance.json)保存源码、索引、fixture SHA256；
[fixture](fixtures/task_144.json)标记 `source_assertion` 与 `runtime_go_oracle=false`。

`hot_region.go:ServeHTTP` 委托 `hot_region_config.go`，GET `/list` 返回
getValidConf有效参数，POST `/config` 深拷贝当前原始参数后覆盖、校验、发布。
校验错误400，JSON解码/读流错误500，均为JSON字符串。相同配置200纯文本；
未知字段且无变化400纯文本。更新200纯文本，**保存失败仍成功且保留新内存值**。
方法错误405空体无Allow，未知路径404文本。BaseScheduler默认不支持分支被覆盖。

`TestUpgrade`覆盖旧配置和优先级回退；`TestReloadConfigKeepsPendingWeightDefault`
覆盖缺失pending-weight保持默认；`TestCreateSchedulerRejectsPendingWeightBelowOne`
覆盖0、0.5、-1拒绝与1接受。`TestGCPendingOpInfos`是pending回收，
`TestSplitIfRegionTooHot`是桶拆分行为，均不是配置HTTP覆盖证据；对应Rust领域行为
随workspace全测验证。所有这些Go测试只读核实，没有把名称列表当作运行结果。

## 实现及测试

生产入口 [serve](../../../pkg/server/schedule_admin_hot_region.rs)独占借用宿主
HotRegionConfig、StorageEndpoint、ClusterPaths；[接口边界](../../../pkg/server/schedule_admin_hot_region.rs.md)。
逐字段检查Go类型，保留顶层重复键/输入顺序、大小写键匹配与exact-tag查找差异。
能力flag不被POST改写；GET不输出disabled，none方向省略。JSON只归一化键序、
缩进、尾换行和等值数值表示。空/null优先级交给既有领域空列表并在GET修复有效值；
不主张领域原始持久化JSON的nil/empty字节级一致性。

测试使用独立显式注册的 [测试文件](../../../pkg/server/schedule_admin_hot_region_test.rs)：

- `task_144_behavior_cases`：固定HTTP/类型/验证/更新案例，拒绝后状态原子保持，
  以及3份完整能力版本视图与6份更新后完整视图，比较全部字段与Content-Type。
- `task_144_faults_and_body_errors`：真实MemoryStore I/O边界失败、成功响应与新值
  可见、失败reload保持状态、旧磁盘值与恢复、重试后重建、完整/截断JSON后的读流失败。
- `task_144_updated_scheduling`：HTTP更新后真实HotScheduler在v1/v2下分别允许写、
  禁写、只禁读；验证Forbidden或真实region10、HIGH、LEADER、1→2步骤，共6组。

## 验证环境

[阶段脚本](logs/task_144/verify.py)原子mkdir动态领取槽位，所有Cargo及子进程继承
绝对CARGO_TARGET_DIR，所有子进程退出后核对owner并释放锁，保留产物缓存。
既有etcd I/O二进制SHA256
`57b00595142cedefe6048a0584bdc3cdb00b883263af99ea95ea50c131a0cfd5`
只读校验后硬链接复用；[拦截器](logs/task_144/reuse-go/go)拒绝实际Go命令。
这不是新增oracle，不构建Go、不注入failpoint。活动记录见go-reuse.jsonl。

首轮前原fixture路径已失效，重新定位到同哈希既有二进制。red至red4因
另一终端运行 `rm -rf ./target` 持续删除构建产物而失败，未视为行为红测。
[环境事件](logs/task_144/environment-incident.json)记录PID、错误和用户“重新开始”
后中断删除进程并重试的处理。仅本槽位失效fingerprint被清除，未删除其它槽位缓存。

协议缓存重建需要调整 [build.rs](../../../pkg/proto/build.rs)，只提取`.proto`到
新`proto-source`目录，保留旧`source`，避免解压或删除Go文件。固定版本、归档
SHA256和生成入口保持不变；只读缓存归档查找由本任务Go拦截器提供，实际Go
命令仍被拒绝。构建说明见[协议构建](../../../pkg/proto/build.rs.md)。
red5停在缓存获取边界，red6/red7记录测试代码OpKind引用错误；修正后
[red8](logs/task_144/red8.json)实际运行3测试，分别在405/更新响应/HTTP更新200
断言失败，exit101。替换最小实现后[green](logs/task_144/green.json)3测试通过。
随后补充quoted-null布尔值和更新后完整GET快照，再执行最终全部检查。

未穷举Go encoding/json每一种词法错误及数字原始拼写诊断；固定fixture中的
状态、错误字符串和全部响应字段均逐项比较，不忽略错误和操作差异。

## 最终结果：已完成

[全部确切命令、退出码、槽位和原始日志](logs/task_144/final.json)、
[汇总](logs/task_144/final-summary.json)：

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 检查通过 |
| `cargo test -p astersql-pd-server --locked --lib task_144 -- --nocapture` | 0 | 3通过，0失败，0忽略 |
| `cargo test -p astersql-pd-server --locked` | 0 | 82通过，0失败，0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 检查通过 |
| `cargo fmt --all -- --check` | 0 | 检查通过 |
| `cargo check --workspace --locked` | 0 | 检查通过 |
| `cargo test --workspace --locked` | 0 | 1882通过，0失败，1忽略 |

聚焦3个真实测试涵盖93个HTTP案例、9份完整响应视图、持久化/恢复及2种真实读流
故障、6组HTTP配置后真实v1/v2调度；全部通过。workspace既有忽略保留，不以忽略
本任务行为或零测试完成验收。Rust最终验证后未变更。

所有阶段实际动态领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`，
CARGO_BUILD_JOBS=4（早期缓存诊断为2）、RUST_TEST_THREADS=2；锁已释放，缓存保留。
[审计](logs/task_144/audit.json)确认1101份Go/模块文件的集合、哈希与状态均未变化，
两级只读plan、Cargo清单/锁不变，Markdown链接有效，git diff --check退出0。
协议只提取55份.proto，不触碰原缓存source中的Go文件。用户终端独立删除缓存的
事件与早期无效编译结果分开保留，未作为行为证据。

仅提交本任务模块、独立测试、lib注册、必要协议构建只读修复和证据；其它已有
编号任务删除保持原样且排除提交。验收后标记并删除144编号任务，不修改原主题。

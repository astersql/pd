# 任务30：同步历史环缓冲

起点 HEAD `354dae6d299d6a54c5abaf05830ce9d861cd6e0a`；task-29累计1–29验收齐备。
已读两份只读plan、任务、AGENTS/Copilot、ADR、依赖基线、移植指南；无更深AGENTS。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`，history_buffer.go及其测试与基准无差异。

## 实现前 oracle

只读审查 history_buffer.go、history_buffer_test.go、metrics.go、client.go/server.go调用方，
core.NewRegionInfo/classifyVoterAndLearner、storage memory backend、kv.NewMemoryKV/Load/Save。
既有测试仅调用内存KV单键Load/Save；mem_kv.go的withRangeLimit位于未到达的LoadRange，
core两个failpoint位于未到达的bucket/subtree路径。显式文件列表排除client/server测试与TestMain，
metrics init只注册Prometheus Gauge；无需注入、文件写入或后台任务。
[入口](task30/oracle.sh)从既有三个Go文件只读编译linux/arm64测试二进制，
GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0，输出只在动态槽位。
[运行结果](task30/oracle.out)：9个顶层测试及5个容量子测试全部PASS，退出0。
其中指定的TestNormalizeHistoryBufferCapacity与TestHistoryBufferKeepsRingBehaviorWithoutRetain
直接提供本任务运行证据；额外retain/持久化测试通过不表示Rust已实现后续任务。

契约：index是下一条记录编号，与MetaStore revision无关；first=index-len；
无retain时固定容量覆盖最旧项；容量按unit乘2归一化，非正unit回到10000。
区间为[start,end)，end夹到next；过旧、未来、反向和空区间返回nil（Rust为空Vec）。
reset清空历史并切到指定next。无计时单位与非确定字段归一化。
区间裁剪和溢出边界区分为源码推导/Rust健壮性测试，不冒充Go事件输出。

[调试入口](task30/snapshot.sh)、[调试命令](task30/oracle.commands)、[真实输出](task30/snapshot.out)：
reset=100，按顺序record区域ID 1、2、3后，h.index=103、head=1、tail=0、size=3，
records的meta.Id为2、3。物理槽位距离为2，因此first=101，逻辑capacity=2。
[解析脚本](task30/parse_oracle.py)从6个实际打印值生成
[fixture](../../../tests/rust-compat/fixtures/state/task-30.json)，Rust直接加载并对齐快照。
调试continue后原测试PASS、进程退出0；没有新增Go工具或修改Go测试。

## 实现与测试驱动

新增canonical `astersql-pd-syncer`，集中登记workspace member/dependency。
生产仅依赖真实core与既有thiserror；serde_json仅作测试fixture加载。
锁文件只新增本地syncer包，generate-lockfile --offline曾提出无关pulldown-cmark升级，
在运行最终--locked检查前已恢复该既有版本，无依赖漂移。

最小可编译接口后运行聚焦测试，[红灯](task30/red.out)实际7 failed、0 passed、退出101：
容量返回0而非10000、追加后next仍100而非103、绕回len为0而非4等真实行为断言。
实现后[绿灯](task30/green.out)7 passed、0 failed/ignored、退出0。

HistoryBuffer使用VecDeque和真实Arc<core::RegionInfo>，HistoryIndex与存储revision独立。
每次追加返回分配索引、next前进，满时覆盖最旧项；归一化、get、半开区间和reset公开可用。
VecDeque无需Go的额外空槽，逻辑容量保持一致；快照持有Arc，覆盖/reset释放环自身引用。
无私有runtime、后台任务、协议依赖或持久化伪实现。后续批次扩展retain/grow/shrink/checkpoint。

7个测试覆盖：Go五组容量和真实环快照；默认/零/非二次幂unit与容量溢出；
空环和前后reset、返回分配索引；多次绕回、部分区间与end夹断；过旧/未来/空/反向区间；
Go已有101个输入在容量1/2/100上的完整记录顺序；索引耗尽错误不改动旧记录、reset后可复用；
快照跨覆盖/reset仍可读，无快照后旧Arc释放。u64::MAX拒绝追加是Rust健壮性增强，
不宣称Go溢出会同样返回错误；负Go容量映射为Rust不接受负值的usize API。


## 最终验证与资源退出

所有Cargo命令及oracle二进制经[动态槽位入口](task30/slot.sh)原子领取槽位1，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
锁目录记录plan/task/session/PID；命令全部结束后释放自己的锁，保留编译缓存。
入口覆盖正常、失败、SIGINT/SIGTERM清理，并在中断时终止/wait整个命令进程组。
本环没有后台任务和后端；Go oracle进程已退出，workspace夹具通过Drop关闭并wait。
已复查proto/build.rs只读固定ZIP缓存，在OUT_DIR生成Rust；
etcd_fixture_test.rs只读编译既有scripts/rust-etcd-fixture/main.go，无需failpoint，
GOFLAGS=-mod=readonly、GOPROXY=off，二进制和隔离数据都落在槽位；EOF关闭etcd并wait。

最终命令入口：`bash docs/rust/state/task30/slot.sh python3 docs/rust/state/task30/validate.py`。
[逐命令输出汇总](task30/validation.out)、[结构化结果](task30/summary.json)与validation-N.out
保存确切命令、退出码、测试数。日志仅去除行末空白用于diff检查，不修改测试数和断言。

| 命令 | 退出码 | 有效测试数 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 格式化无无关Rust差异 |
| `cargo test -p astersql-pd-syncer --locked --lib state_task_30_history_buffer -- --nocapture` | 0 | 7 passed、0 failed/ignored |
| `cargo test -p astersql-pd-syncer --locked` | 0 | 7 passed、0 failed/ignored |
| `cargo metadata --format-version 1 --no-deps` | 0 | 非测试 |
| `cargo fmt --all -- --check` | 0 | 非测试 |
| `cargo check --workspace --locked` | 0 | 非测试 |
| `cargo test --workspace --locked` | 0 | 1146 passed、0 failed、1既有ignored |

既有ignored是tso_processes，本任务没有忽略测试；check仅既有cpu_usage弃用警告。
[只读核验](task30/readonly.out)确认1101个Go/模块文件集合、内容与Git状态未变，
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
[起点哈希](task30/go-before.json)、[起点状态](task30/go-status-before.out)、
[只读计划哈希](task30/plans-before.json)均已保留，所有plan.md未改，文档链接和diff检查通过。
[资源核验](task30/resources.out)确认锁已释放、缓存保留、无槽位产物进程残留。

状态：已完成；只验收批次30的纯领域历史环，不代表同步主题或整个统计主题完成。

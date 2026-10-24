# 任务46：心跳副作用顺序与容错

实施前已读两份只读plan、AGENTS、Copilot与Rust架构/依赖/移植指南。
依赖[task-45](task-45.md)已完成，累计前置沿该记录；无更深AGENTS。
起点commit、1101项Go/模块逐项SHA256和Git状态、只读plan哈希见
[before.json](task46/before.json)。Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0。

## 实施前oracle与契约

只读复用task25编译、task41复用的Linux/ARM64 cluster-oracle41.test，
SHA256 8781f08225e2b8b746217d79fe78e3ff072369cc8ae8941cc8d616a8a77986d9。
审查TestMain、newTestRaftCluster、ContextTODO/SyncRunner、内存storage与
processRegionHeartbeat调用链；所选测试不需要failpoint，容器仅挂载既有二进制。
[oracle.out](task46/oracle.out)记录TestRegionHeartbeat、TestHeartbeatSplit、
TestOfflineAndMerge真实通过，包含缓存/KV断言；后续加入TestRegionSizeChanged重跑。
只读源码是server/cluster/cluster.go:1344–1492、pkg/cluster/cluster.go:45–95、
pkg/core/region.go的guide、pkg/ratelimit/runner.go；未将源码分析当作运行差分。

输入：新region(epoch2/2)、重复、version/confver增加、过期报告、down/pending、peer数变化、
leader变化、size/keys/IA size变化、读写流量、flashback，以及split/merge重叠。
新region/epoch变化保存缓存和KV并同步；重复不再写KV或同步；leader/流量变化
更新缓存并同步、不写KV；size/flashback仅缓存与统计。持久化只含RegionMeta，
size单位MiB、keys为计数、时间宿主Unix秒；history index独立于MetaStore revision。
提交顺序为root→subtree→overlap清理→统计→KV→同步，但三个runner间没有完成顺序保证。
统计执行时从root重读当前region，消失则跳过；duplicate按当前配置重新判定特殊分类。
subtree duplicate仅在业务ref<2时修复并retained=true；cache更新采用guide.retained，
同步始终retained=true。队列拒绝和存储错误不能回滚已接受root。

必要存储错误运行证据：[调试输入](task46/failure.commands)、
[最终输出](task46/failure-clean.out)。Delve仅在EncryptRegion入口把该次存储参数设nil，
触发真实SaveRegion错误；heartbeat原始region和root/subtree仍存在，返回后changedRegions长度1。
这是持久化参数故障边界，不是磁盘I/O故障或原TestRegionHeartbeat全通过；Rust另测真实Etcd断连。
初次使用过期storage.go行号失败；随后修正断点与regionItem嵌入字段名，保留初次日志以供追溯。
EOF/退出回收调试进程，具名Docker容器EXIT/INT/TERM清理。

## 红灯、实现与行为快照

[red.out](task46/red.out)：可编译入口只提交root，不执行effects；1项实际执行，
0通过/1失败/0忽略，退出101，业务引用数实际1、预期2。最初编译阶段的四个接口名
不匹配已按现有API修复，见red-initial.out；不将编译错误作为行为红灯。
[green-initial.out](task46/green-initial.out)首次1通过，
[green-expanded.out](task46/green-expanded.out)扩展8通过；
[green-final-fixed.out](task46/green-final-fixed.out)10通过/0失败/0忽略。

新增heartbeat_effects.rs的submit_region_heartbeat、HeartbeatEffectsState、HeartbeatRunners、
HeartbeatEffectsReceipt与类型化effect故障。region_heartbeat.rs仅抽出crate-private
commit_root复用原准备/继承/guide/重查，原同步入口行为不变。
不重复runner、索引、分类、规则计算或history；不引入协议、调度决策、全局状态或私有runtime。
Cargo只增加现有workspace依赖边，无新第三方版本。模块和独立测试在lib.rs显式登记。

10项测试的实际快照覆盖：

- 新region：root/subtree业务ref2，KV含RegionMeta，history一条；duplicate保留同一root Arc，
  KV/history不再写；leader与flow新增history，size/flashback不写KV/history。
- bucket变化即使RegionMeta相同仍实际增加KV revision；peer数改变保存真实metadata；
  epoch更新retained=true，stale报告保持root Arc、KV revision、history长度不变。
- root-only duplicate修复subtree且不补KV/history；max merge size配置20→5→20使
  UndersizedRegion true→false→true，guide仍全false。
- MemoryStore与真实EtcdMetaStore：全范围region1 v2→右半region2 v3→左半region1 v3→
  合并region1 v4，最终root/KV仅region1，history顺序(1,2),(2,3),(1,3),(1,4)，
  overlap分类删除，label延迟清理后计数0，最终业务ref2。
- 受控MemoryStore写失败边界：删除region2失败但继续删除region3并保存region1；
  下一次save失败后root/history为v4、KV仍v3。成功操作由真实MemoryStore完成。
- 真实etcd停止后保存v3失败，root/subtree接受v3、history追加；重启后KV仍v2；
  duplicate v3不重试失败保存，root/history不回滚。
- limiter真实token阻塞并取消三个runner后4项completion均Cancelled，root ref1，
  KV/history空；重启runner后duplicate修复ref2，不伪造持久化或history。
- aged FIFO下新region的retained subtree/sync成功，size-only subtree因age拒绝，
  root仍接受size144。停止后的4项提交返回Stopped，root仍接受更高epoch。
- 延迟Collect使用root最新v3/size144，分类为Oversized，而该次KV/history仍是提交的v2。

第一次年龄测试错误假设set_limit会唤醒已等待的token，验证进程被主动终止（SIGTERM，
slot.sh终止并等待整个自有进程组后释放锁）；green-final.out保留该次未完成输出。
修正为预先取得真实token并release_token唤醒，不修改依赖limiter，最终10项全部通过。
每个测试显式stop等待三个runner；取消场景验证waiting/running归零并允许重复stop。
Etcd夹具EOF关闭、wait回收，异常时kill后wait，测试目录正常退出时删除。

已审查既有etcd_fixture_test.rs→scripts/rust-etcd-fixture/main.go，仅只读go build；
GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0、GOARCH=arm64，产物在持有槽位。
proto/build.rs核对固定ZIP摘要，仅在OUT_DIR解压和生成。未运行make、failpoint注入、
Go格式化或mod tidy，未新增或改写任何Go源码/模块。

## 最终验证与退出

主体入口：`bash docs/rust/state/task46/slot.sh bash -c 'bash docs/rust/state/task46/oracle.sh > docs/rust/state/task46/oracle-final.out 2>&1 && python3 docs/rust/state/task46/validate.py'`。
最终聚焦入口：`bash docs/rust/state/task46/slot.sh python3 docs/rust/state/task46/focus-final.py`。
[逐命令统计](task46/summary.json)、[主体阶段](task46/validation.out)、
[最终聚焦阶段](task46/focus-final.out)及validation-0.out至validation-6.out保存结果。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 无范围外Rust格式差异 |
| `cargo test -p astersql-pd-cluster --locked --lib state_task_46_heartbeat_effects -- --nocapture` | 0 | 10通过/0失败/0忽略 |
| `cargo test -p astersql-pd-cluster --locked` | 0 | 43通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 成功 |
| `cargo fmt --all -- --check` | 0 | 成功 |
| `cargo check --workspace --locked` | 0 | 成功 |
| `cargo test --workspace --locked` | 0 | 1267通过/0失败/1既有忽略 |

既有忽略为tso_processes；本任务无忽略或零测试放行。主体阶段误用继承的
state_task_46_bucket_heartbeat过滤器运行了零测试，该次输出独立保留为validation-1-zero.out、
focus-zero.json；修正validate.py并增加聚焦测试数断言后，focus-final.py重新运行正确过滤器，
将summary.json的对应项与validation-1.out更新为10项真实通过。
本次没有Rust代码变更发生在最终fmt/包级/workspace验证与最终聚焦验证之间。
[oracle-final.out](task46/oracle-final.out)4项原Go测试通过，退出0；
[failure-clean.out](task46/failure-clean.out)完成故障输入后缓存、sync快照读取，无调试命令错误。
通用workspace还通过真实MemoryStore与EtcdMetaStore契约，新增测试另有自身后端快照证据。

所有阶段动态领取槽位2，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
所有Cargo/Go夹具/调试子进程继承绝对槽位，无target-dir覆盖；锁内owner记录task46与PID。
[资源核对](task46/resources.json)：自有锁已释放，槽位缓存保留，无槽位产物进程或自有oracle容器；
未修改其他会话槽位1。
[只读核对](task46/readonly.json)：1101项Go/模块集合、逐项SHA256及Git状态与起点相同，
聚合SHA256为ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4；
两份指定plan.md哈希不变。无Go或模块文件增删改，未纳入用户既有计划变更。
输出仅规范制表符与行尾空白；git diff --check与证据链接检查通过。

状态：已完成。仅完成心跳副作用领域边界，不代表完整调度服务或整个来源主题完成。

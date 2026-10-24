# 任务45：桶报告版本与竞争

依赖[task-44.md](task-44.md)已完成，累计前置沿该记录。
已读两份只读plan、AGENTS、Copilot、ADR、依赖基线和移植指南，无更深AGENTS。
起点commit及1101项Go/模块SHA256、Git状态、两份plan哈希见[before.json](task45/before.json)。
Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0。

## 实施前oracle

复用既有Linux/ARM64 cluster-oracle41.test（由task25只读编译，task41复用），
SHA256 8781f08225e2b8b746217d79fe78e3ff072369cc8ae8941cc8d616a8a77986d9。
已审查TestMain、newTestRaftCluster、内存storage、核心版本比较、HandleRegionBuckets与worker调用链。
普通TestBucketCompatibility、TestStaleBucketMeta、TestBucketHeartbeat不需要failpoint，
只读挂载执行[oracle.out](task45/oracle.out)，3通过，退出0，先于实现。

输入与快照：region1，[a,d)，report v2；无bucket metadata的region epoch更新继承v2，
独立report v3后继承v3；heartbeat metadata v4优先显示，独立report v5仍单独保存。
分裂后范围[c,d)，旧report边界可保留，后续heartbeat metadata v3替换显示边界。
缺失region报错；同版本成功、v3替换v1；关闭bucket inheritance后缺失报告不再继承。

必要竞争运行证据：[调试输入](task45/concurrent.commands)、[脚本](task45/concurrent.sh)、
[输出](task45/concurrent.out)。在原TestConcurrentReportBucket的v3到达真实CAS指令前，
Delve调用os.ReadFile读取容器内FIFO；writer在读端打开后等待500ms再写入并关闭。
原测试主goroutine在100ms后提交v2。暂停的v3仍持有old=nil，而实际reportBuckets=v2；
继续执行真实CAS失败并重试，第二次old=v2、candidate=v3，原测试最终断言v3并PASS。
未改Go源码或生成注入，未将没有注入的普通执行称为竞争覆盖。
原time.Sleep调试调用因optimized regabi被拒绝；首个FIFO版本未延迟、第二个重复writer
不能可靠EOF而被主动停止；最终单次writer路径无调试错误，容器退出回收。
这是调试控制的真实发送暂停边界，替代原failpoint睡眠，不是源码推导。

三次重试运行证据：[retry.commands](task45/retry.commands)、[retry.out](task45/retry.out)。
在真实CAS前将old设为真实candidate指针（当前slot仍nil），造成真实指针比较失败；
连续三次后到达cluster.go:1338返回false,nil，region.reportBuckets仍nil。
不把调试场景称为未经修改输入的原测试通过。

字段契约：core RegionBuckets只含region_id/version/keys；完整BucketReport的flows与period
原样交worker，period单位毫秒，worker换算每秒负载；heartbeat metadata优先于report。
低/相等版本core返回成功但保留原report，Go集群仍将这类成功报告交worker。
只有missing region错误、Removed leader及CAS三次失败不入worker；缺失leader/store不拒绝。
宿主显式传入可选worker以表达是否本地处理热点，领域不依赖协议或调度。
Rust core独占借用使单次compare_and_set不可被中途写入打断；保留三次重试策略，
不伪造生产CAS失败。策略测试用真实core.update_buckets指针竞争验证0至3次冲突。

补充Removed运行快照：[tombstone.commands](task45/tombstone.commands)、
[tombstone.out](task45/tombstone.out)。在原TestBucketHeartbeat已经注册stores后，仅改变
进程内store metadata为Tombstone(2)/Removed(3)，真实processRegionBuckets在1325行
返回false,nil，reportBuckets仍nil。首次尝试改变原stores切片未作用于注册快照；
最终直接改变缓存中的metadata，无Go文件写入。这是修改输入的运行快照，非原测试全通过。

## 红灯与实现

[red.out](task45/red.out)：最小可编译入口总返回BucketRegionNotFound，重试策略只执行一次；
7项实际执行，0通过/7失败/0忽略，退出101。已注册region却返回错误，以及重试次数1不等于2，
均为真实断言失败，不是编译错误或零测试。之前同阶段cargo fmt/check成功并维护锁文件。

新增bucket_heartbeat.rs::handle_bucket_heartbeat与BucketRegionNotFound；
lib.rs显式注册生产和独立测试模块。复用现有core版本语义和BucketWorkerClient，
更新同时维护root/role引用与统计，同/低版本不替换缓存Arc。false不提交热点。
不新增调度决策、协议依赖、持久化或私有运行时。
Cargo只增加两个已有workspace依赖边：生产statistics-buckets、测试tokio-util，
Cargo.lock无新第三方包。行为说明见相邻bucket_heartbeat.rs.md。

Rust core在宿主锁下串行CAS，不能中途失败；未声称复现Go乐观写入时的失败窗口。
保留最多三次策略，以真实core.update_buckets的过期Arc与当前Arc验证：
0/1/2次冲突后成功，3次冲突返回false且不进行第四次；状态保持实际赢家的v2。
跨线程门闸验证v3/v2两种提交顺序最终均为v3，全部线程join。
真实worker验证full payload FIFO与毫秒间隔，missing/Removed后collect空，
关闭worker后提交保留core接受结果；每个worker显式shutdown取消并join，重复关闭安全。
其余测试覆盖metadata优先、分裂边界继承、禁用继承、未知leader/store和索引引用计数。

[green.out](task45/green.out)首次7通过/0失败/0忽略，退出0；
随后加强root/role业务引用计数断言再执行完整验证。

## 最终验证与退出

入口：`bash docs/rust/state/task45/slot.sh python3 docs/rust/state/task45/validate.py`。
[逐命令统计](task45/summary.json)、[阶段输出](task45/validation.out)与validation-0.out至
validation-6.out保存结果。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 无范围外Rust格式差异 |
| `cargo test -p astersql-pd-cluster --locked --lib state_task_45_bucket_heartbeat -- --nocapture` | 0 | 7通过/0失败/0忽略 |
| `cargo test -p astersql-pd-cluster --locked` | 0 | 33通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 成功 |
| `cargo fmt --all -- --check` | 0 | 成功 |
| `cargo check --workspace --locked` | 0 | 成功 |
| `cargo test --workspace --locked` | 0 | 1257通过/0失败/1既有忽略 |

既有忽略为tso_processes，本任务没有忽略。workspace包括真实MemoryStore与EtcdMetaStore
契约；本同步函数无持久化/监听，不把通用后端结果当作新增持久化能力。
已审查etcd_fixture_test.rs与现存scripts/rust-etcd-fixture/main.go调用链，仅只读go build，
GOFLAGS=-mod=readonly、GOPROXY=off、CGO_ENABLED=0，产物进入持有槽位；EOF关闭和wait回收，
失败时kill后wait。proto/build.rs校验固定ZIP后只在OUT_DIR处理生成，不改仓库Go/模块。
没有运行make、failpoint启停、Go生成/整理/格式化命令。

红灯、最终基线、主要oracle动态领取槽位2，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
首次聚焦绿灯时槽位2仍在运行只读调试容器，动态领取槽位3，
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-3`。
所有Cargo命令继承绝对路径，未传target-dir覆盖。自有子进程退出后释放各阶段自有锁，
缓存保留，未修改他人槽位1。调试脚本最终增加具名容器EXIT/TERM/INT清理，并重新执行
全部oracle验证；[oracle-final.out](task45/oracle-final.out)阶段退出0，三项原测试通过、
真实v3/v2竞争后PASS、三次CAS失败与Removed均返回false,nil，无最终调试命令错误。
脚本由slot.sh启动，容器只读挂载源码外的既有二进制与输入文件；退出回收debuggee。
[资源核对](task45/resources.json)确认自有锁释放、无使用槽位产物的进程和oracle容器、缓存保留。

[只读核对](task45/readonly.json)：1101项Go/模块的集合、逐项SHA256及Git状态与起点一致，
聚合SHA256为ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4；
两份指定plan.md哈希不变，Go参考文件与固定基准无差异。
证据输出只规范制表符和行尾空白。git diff --check、Markdown证据链接核对通过。
用户既有计划文件变更不纳入提交。

状态：已完成。范围仅为桶报告状态面，非完整心跳服务或整个来源主题完成。

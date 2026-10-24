# 任务43：区域心跳接受与重判

依赖任务42已完成（docs/rust/state/task-42.md），累计前置沿其记录；
已读两份只读plan、AGENTS、Copilot、ADR、移植指南和依赖基线，无更深AGENTS。
起点commit、1101项Go/模块内容和只读plan哈希保存于[before.json](task43/before.json)。
Go基准69ba16e6d47dfe59a21498c21641e1c1eaa726e0；对应Go源与基准无差异。

## 实施前运行证据

复用既有Linux/ARM64二进制cluster-oracle41.test，SHA256
8781f08225e2b8b746217d79fe78e3ff072369cc8ae8941cc8d616a8a77986d9。
审查TestMain、内存storage、newTestRaftCluster、上下文取消与runUpdateStoreStats调用链，
选中TestRegionHeartbeat/RegionFlowChanged/RegionSizeChanged/CheckStaleRegion不依赖failpoint，
无Go/模块/生成写入。通过[oracle.sh](task43/oracle.sh)只读挂载运行，
[oracle.out](task43/oracle.out)4通过，退出0。

事件顺序来自cluster_test.go:1095–1430、2351：新region、重复、version增加、
旧version/旧confver拒绝、增删down/pending/peers、换leader、size/keys/IA大小与flow变更、
flashback、低版本overlap拒绝和新overlap逐出。FlowChanged报告read_bytes=1000，根树保留1000；
SizeChanged跨merge阈值变更缓存（阈值统计副作用留给下一任务）。
size在领域中为MiB、keys为整数、flow为报告累积字节，epoch/term为uint64。
CheckStaleRegion覆盖(0,0)/(1,0)/(1,1)/(0,1)逐分量比较。

原TestConcurrentRegionHeartbeat需要源码failpoint注入，未裸跑该测试冒充并发验收。
改用既有无注入二进制和Delve，在TestRegionHeartbeat更新version的调用中，
到达cluster.go:1406（已过PreCheck/Inherit/guide）后只set进程内origin.version=100；
[recheck.out](task43/recheck.out)显示当前2、报告3，然后提交返回stale，当前100、报告3。
这是控制提交边界的运行快照，不宣称原并发测试通过或同ID全序。
[命令](task43/oracle.commands)、[脚本](task43/debug.sh)可重放；初次断点路径歧义见
[初次输出](task43/recheck-initial.out)，修正为绝对源路径后快照无命令错误。
Docker --rm与Delve exit关闭debuggee，不改Go、不注入、不创建Go工具。

源码补充契约明确区分运行差分：core.check对nonzero旧term、任一旧epoch拒绝，
term0未知；非全范围epoch(1,1)允许unsafe recovery；guide忽略term，重复不写root，
root提交再次查当前overlap；Go同region并发仍有最后写入窗口。
异步hot/statistics/storage/sync等副作用属于下一任务，本任务只返回guide和overlaps。

补充term运行快照：[term.commands](task43/term.commands)在同一提交断点仅设置
origin.term=5、region.term=4，真实Go返回stale，见[term-final.out](task43/term-final.out)。
这是提交边界的补充运行证据；不把控制内存的输入当作原测试完整通过。

## 红灯、实现与行为

[red.out](task43/red.out)：最小可编译prepare/commit不更新真实状态，6项执行、
0通过/6失败/0忽略，退出101；明确失败包括新region guide未要求save、
预检之后的旧报告提交仍成功、overlaps为空而期望[1]。非缺符号或零测试。

新增region_heartbeat.rs，公开prepare_region_heartbeat、PreparedRegionHeartbeat::commit、
handle_region_heartbeat和RegionHeartbeatResult，显式注册生产/测试模块。
prepare复用真实BasicCluster.regions.pre_check_put_region、RegionInfo.inherit与region_guide。
commit对save_cache路径调用check_and_put_root_tree再次校验，返回真实逐出集；
check_and_put_sub_tree重新从当前root取值，不会用迟到事件复活已逐出的ID。
重复路径不更新root，同时修复root-only子树。返回guide供下一批接线持久化、统计和sync。
Go同epoch并发最后提交可覆盖统计；term-only不强制缓存写入；不声称提供更强全序。
无依赖、feature或Cargo.lock变更；领域不引入protobuf、存储、调度或私有runtime。
生产文件有版权与相邻rs.md，lib.rs.md同步入口说明。

[初次绿灯](task43/green.out)5通过/1失败：测试把epoch2的新region置于已有epoch3的全范围
region内，被真实core正确拒绝。改为独立空集群测试默认size=1，未放松stale规则。
[修正后绿灯](task43/green-final.out)9通过/0失败/0忽略，退出0。

9项行为检验新建/重复/size+keys和bucket继承、旧version/confver/nonzero term拒绝、
term0和term-only、read_bytes=1000与size144/keys144000/IA40、leader变化、
准备后overlap合并拒绝、same-ID version重判及同epoch逆序覆盖、重复补全子树、
真实两线程预检—合并—提交拒绝、已逐出重复不复活、down/pending/peer变化与unsafe recovery。
全部使用真实core对象并检查根树/角色子树/计数；无mock索引或统计。
并发用宿主Mutex包裹BasicCluster，prepare释放锁后由通道控制合并时序，
线程显式join，通道等待有超时，不使用sleep猜测提交顺序。
本领域没有持久化或监听；MemoryStore/EtcdMetaStore的通用契约由workspace真实回归验证，
不将它们冒称为该函数的持久化实现。

## 最终验证与资源

入口：`bash docs/rust/state/task43/slot.sh python3 docs/rust/state/task43/validate.py`。
[逐命令计数](task43/summary.json)、[阶段输出](task43/validation.out)、validation-0.out至
validation-6.out保存全部结果（输出仅统一制表符与尾部空白）。代码变更后先cargo fmt --all，无范围外Rust差异。

| 命令 | 退出码 | 有效测试 |
| --- | --- | --- |
| `cargo fmt --all` | 0 | 成功 |
| `cargo test -p astersql-pd-cluster --locked --lib state_task_43_region_heartbeat -- --nocapture` | 0 | 9通过/0失败/0忽略 |
| `cargo test -p astersql-pd-cluster --locked` | 0 | 21通过/0失败/0忽略 |
| `cargo metadata --format-version 1 --no-deps` | 0 | 成功 |
| `cargo fmt --all -- --check` | 0 | 成功 |
| `cargo check --workspace --locked` | 0 | 成功 |
| `cargo test --workspace --locked` | 0 | 1245通过/0失败/1既有忽略 |

既有忽略为tso_processes，本任务无忽略。基线真实执行MemoryStore与EtcdMetaStore契约。
审查proto/build.rs仅在OUT_DIR解压固定kvproto ZIP并生成Rust；
etcd_fixture_test.rs仅用GOFLAGS=-mod=readonly构建现有Go夹具，无failpoint依赖，
夹具EOF请求关闭etcd并wait，panic路径亦清理。未运行会改Go的make或注入命令。

所有Cargo、成功oracle与最终回归动态领取**槽位2**：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-2`。
一次早期term调试探测领取槽位3：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-3`，
因该槽位没有既有oracle二进制而未取得有效快照；释放后最终term快照在槽位2重放。
[资源核验](task43/resources.json)：自有子进程全部结束，自有锁已释放，缓存保留，
未操作其他任务槽位1。无新增长期后台任务；并发线程显式join。

[只读核验](task43/readonly.json)与[重放脚本](task43/readonly.py)：1101项Go/模块清单与
内容逐项等于起点，聚合SHA256为
`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`，
Go Git状态为空，两份指定plan.md哈希不变，无新增Go/模块文件。
`git diff --check`通过，文档相对链接均可解析；用户已有计划变更保留。

状态：已完成。范围为region心跳状态接受与重判，异步副作用仍由后续任务接线。

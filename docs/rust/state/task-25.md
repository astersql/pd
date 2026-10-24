# 任务25：热点查询结果投影

起点HEAD `a177aad73712d15133a60a10bdb5e05c599853b1`，task-24累计依赖已完成。
已阅读两份只读plan、AGENTS、Copilot、ADR、依赖基线、移植指南；无更深AGENTS。
Go基准 `69ba16e6d47dfe59a21498c21641e1c1eaa726e0`；三份Go参考文件无后续差异。

## 实现前oracle

只读审查store_hot_peers_infos.go、hot_regions_stat.go、store_load.go、
hot_peer.go及coordinator.GetHotRegionsByType。现有cluster.TestCollectMetrics调用两种投影，
原测试无store，不能单独作为非空投影oracle。
审查该测试prepare/newTestCluster/newTestRaftCluster、内存storage、addRegionStore/setStore、
TestMain/WaitForEtcdConnections/goleak：选择路径不需要failpoint，不写Go/模块。
只读编译命令（经task25/slot.sh领取槽位）：
`GOOS=linux go test -mod=readonly -c -gcflags="all=-N -l" -o "$CARGO_TARGET_DIR/cluster-oracle25.test" ./server/cluster`。
编译退出0。使用Alpine 3.22/edge Delve在该既有测试3022行暂停，
只改变进程内存输入，通过真实Go函数捕获返回值与encoding/json.Marshal字节。
[确切调试输入](task25/oracle.commands)、[实际输出](task25/oracle-snapshots.out)、
[解析后JSON](../../../tests/rust-compat/fixtures/state/task-25.json)。
早期探测的variadic调用缺少nil参数，保留oracle-initial.out；最终输出无命令错误。

输入：新增store1/2/3；截取regionStats[1]前三项、[2]第一项、[3]空。
将region ID固定11/12/13/21；11为leader且loads=[11.4,12.6,-1.5,3.5]；
12/13热度0/-1，其余loads=[10,20,30,0]；21热度10 follower。
Collect保留非正热度，store1总量=[31,53,58,4] count3，leader=[11,13,-2,4] count1；
GetHotStatus排除非正热度，store1总量=[11,13,-2,4] count1。
write leader store rate来自全部leader peer，write region当前store rate为0。
空输入maps为空；已知空peer集合仍保留store3；Collect statistics=null，详细statistics=[]。
GetHotStatus默认learner=false，更新时间为Go零time的RFC3339字符串（omitempty仍输出）。
第三快照只将store2输出项IsLearner置true，证明适配层补充后JSON字段契约。
这是实际函数和JSON运行证据；没有把源码推导标为Go运行差分。
本任务同步值投影，不需要存储/后台生命周期，复用批次24领域聚合。

## 实现与TDD

新增hot_projection.rs、hot_projection_test.rs及相邻行为说明，lib.rs显式注册并导出：
HotPeersStat、HotPeerStatShow、StoreHotPeersInfos、StoreHotPeersStat、
collect_hot_peer_infos、get_hot_status与StoreLoadDetail::to_hot_peers_stat。
只增加workspace已有serde/serde_json依赖；Cargo.lock仅增加本crate的两条依赖记录。
首次offline测试维护锁文件，但因测试局部借用冲突编译失败（initial-compile.out），
修正测试借用后，正式`--locked`红灯0 passed/5 failed、退出101：
实际断言为ID集合[]而期望[1,2,3]、current rates全0而期望[100.25,10.5,5.75,0.25]。
[红灯](task25/red.out)不是编译错误或零测试；实现后相同聚焦命令
[绿灯](task25/green.out)退出0、5 passed/0 failed/0 ignored。

额外Rust断言覆盖未知store忽略、缺失load、空peer保留、cold-only无详细项、
分数store rates不取整、逐项负载取整、future不污染current、owned stores隔离、
u64::MAX region ID精确JSON整数保留。三个Go快照逐字段比较，只对数字token的
整数/浮点写法按数值比较；不忽略字段、null/[]、数组顺序或布尔/时间差异。
输入领域空stores按未设置输出null；调用者可以显式设置Some(empty Vec)输出[]。
learner/更新时间遵循Go统计层与上层适配的职责分界，没有新增HTTP/集群反向依赖。

## 验证与资源

[重现oracle脚本](task25/oracle.sh)须通过槽位包装器调用；
[解析器](task25/parse_oracle.py)核对原始Marshal字节与fixture，退出0，3快照完全一致。
所有构建与验证阶段均动态mkdir领取槽位1：
`CARGO_TARGET_DIR=/Users/Shared/work/dir/data/codes/pd/target/rust-slot-1`。
[槽位包装器](task25/slot.sh)记录任务/计划/会话/PID，信号或正常结束均先终止/wait子进程组再释放锁。
Go环境继承GOARCH=arm64、CGO_ENABLED=0、GOFLAGS=-mod=readonly、GOPROXY=off。
Delve调试入口仅运行既有测试选择路径；exit终止整个debuggee，Docker --rm回收容器。
没有把调试执行称为原测试通过；原测试的默认输入没有足够非空业务覆盖。
已复核pkg/proto/build.rs只在OUT_DIR生成、固定ZIP校验；
etcd_fixture_test.rs调用既有scripts/rust-etcd-fixture/main.go，go build只写槽位，
后端数据隔离于槽位，EOF关闭及Drop kill/wait兜底，无failpoint与Go源码写入。

[完整验证](task25/validation.out)、[逐命令统计](task25/summary.json)：

| 命令 | 退出码 / 有效测试数 |
| --- | --- |
| `cargo fmt --all` | 0；无范围外格式差异 |
| `cargo metadata --format-version 1 --no-deps` | 0；metadata.out |
| `cargo fmt --all -- --check` | 0 |
| `cargo test -p astersql-pd-statistics --locked --lib state_task_25_hot_projection -- --nocapture` | 0；5 passed、0 failed/ignored |
| `cargo test -p astersql-pd-statistics --locked` | 0；94 passed、0 failed/ignored |
| `cargo check --workspace --locked` | 0；仅既有cpu_usage弃用警告 |
| `cargo test --workspace --locked` | 0；1109 passed、0 failed、1既有ignored |
| `python3 docs/rust/state/task25/parse_oracle.py` | 0；3实际Go JSON快照 |
| `python3 docs/rust/state/task25/readonly.py` | 0；文件集合/哈希/状态/计划/链接/diff检查 |

workspace包括真实MemoryStore/EtcdMetaStore契约；既有ignored为tso_processes。
[只读核对](task25/readonly.out)确认1101个Go/模块文件集合、SHA256和Git状态不变，
aggregate SHA256=`ae4ea1307f6a0074d7d6b706be224a1d139607a343ed2b2c6ea9068395425bd4`。
起始go-before.json、go-status-before.out和readonly-plans.json保留，可重复核验。
[资源核对](task25/resources.out)：自己的槽位锁已释放、无进程使用槽位，缓存保留；
未修改两份只读plan或用户已有变更。

状态：已完成；只验收本批次热点查询值投影，不表示整个统计主题完成。

证据日志仅去掉行末空白及多余末尾空行，未改写Go JSON字节或断言内容。
